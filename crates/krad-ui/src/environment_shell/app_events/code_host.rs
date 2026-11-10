use gpui_kit::{Context, Task};
use krad_domain::review::{MergeRequestQuery, ProjectPath};
use krad_domain::{code_host_events, CodeHost, CodeHostSnapshot, EnvironmentName, GroupId};
use krad_gitlab::GitlabConfig;
use krad_host::HostLink;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

use super::super::review_view::{origin_repo, HostClient};
use super::super::EnvironmentShell;

const CODE_HOST_POLL: Duration = Duration::from_secs(120);
const MAX_WATCHED_REQUESTS: usize = 20;

type ProjectKey = (CodeHost, String, ProjectPath);

struct WatchTarget {
    environment: EnvironmentName,
    group: GroupId,
    root: PathBuf,
    link: HostLink,
}

struct Observed {
    environment: EnvironmentName,
    group: GroupId,
    project: ProjectKey,
    snapshot: CodeHostSnapshot,
}

#[derive(Default)]
pub(in crate::environment_shell) struct CodeHostWatch {
    snapshots: HashMap<ProjectKey, CodeHostSnapshot>,
    task: Option<Task<()>>,
}

fn snapshot_of(client: &HostClient, project: &ProjectPath) -> Option<CodeHostSnapshot> {
    let open = client
        .merge_requests(project, MergeRequestQuery::default())
        .map_err(|error| tracing::debug!(%error, "open merge requests not read"))
        .ok()?;
    let mut snapshot = CodeHostSnapshot::default();
    for request in open.iter().take(MAX_WATCHED_REQUESTS) {
        match client.pipelines(project, request.iid) {
            Ok(pipelines) => snapshot.insert(request.iid, &pipelines),
            Err(error) => {
                tracing::debug!(%error, "pipelines not read");
                snapshot.insert_unread(request.iid);
            }
        }
    }
    Some(snapshot)
}

#[tracing::instrument(skip_all, fields(groups = targets.len()))]
fn observe(targets: Vec<WatchTarget>, gitlab: Option<&GitlabConfig>) -> Vec<Observed> {
    let mut seen: HashSet<ProjectKey> = HashSet::new();
    let mut observed = Vec::new();
    for target in targets {
        let Some(repo) = origin_repo(&target.link, &target.root, gitlab.cloned()) else {
            continue;
        };
        let Ok(Some(client)) = repo.login.client() else {
            continue;
        };
        let project: ProjectKey = (client.code_host(), client.host().to_owned(), repo.project);
        if !seen.insert(project.clone()) {
            continue;
        }
        let Some(snapshot) = snapshot_of(&client, &project.2) else {
            continue;
        };
        observed.push(Observed {
            environment: target.environment,
            group: target.group,
            project,
            snapshot,
        });
    }
    observed
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn start_code_host_watch(&mut self, cx: &mut Context<Self>) {
        let task = cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(CODE_HOST_POLL).await;
            let Ok(wanted) = this.update(cx, |shell, _| shell.code_host_targets()) else {
                break;
            };
            let Some((targets, gitlab)) = wanted else {
                continue;
            };
            let observed = cx
                .background_executor()
                .spawn(async move { observe(targets, gitlab.as_ref()) })
                .await;
            if this
                .update(cx, |shell, cx| {
                    shell.apply_code_host(observed);
                    shell.flush_kraddies(cx);
                })
                .is_err()
            {
                break;
            }
        });
        self.app_events.code_hosts.task = Some(task);
    }

    fn code_host_targets(&self) -> Option<(Vec<WatchTarget>, Option<GitlabConfig>)> {
        let wanted = self.settings.webhooks.wants_code_host()
            || self.wants_code_host_events()
            || self.automations_want_code_host();
        if !wanted {
            return None;
        }
        let targets = self
            .order
            .iter()
            .filter_map(|name| Some((name, self.envs.get(name)?)))
            .flat_map(|(name, live)| {
                live.data.groups.iter().map(|group| WatchTarget {
                    environment: name.clone(),
                    group: group.id,
                    root: group.root.clone(),
                    link: live.link.clone(),
                })
            })
            .collect();
        Some((targets, GitlabConfig::try_from(&self.settings.gitlab).ok()))
    }

    fn apply_code_host(&mut self, observed: Vec<Observed>) {
        for seen in observed {
            let snapshots = &mut self.app_events.code_hosts.snapshots;
            let events = code_host_events(snapshots.get(&seen.project), &seen.snapshot);
            let host = seen.project.0;
            let kept = seen.snapshot.carried_over(snapshots.get(&seen.project));
            snapshots.insert(seen.project, kept);
            for event in events {
                self.observe_code_host_event(&seen.environment, seen.group, host, &event);
                self.publish_event(&seen.environment, Some(seen.group), event);
            }
        }
    }
}
