use krad_domain::review::{MergeRequestQuery, ProjectPath};
use krad_domain::{
    code_host_events, AppEvent, CodeHost, CodeHostSnapshot, EnvironmentName, GroupId,
};
use krad_gitlab::GitlabConfig;
use krad_host::HostLink;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::{origin_repo, HostClient};

const CODE_HOST_POLL: Duration = Duration::from_secs(120);
const MAX_WATCHED_REQUESTS: usize = 20;

type ProjectKey = (CodeHost, String, ProjectPath);

#[derive(Debug)]
pub(crate) struct WatchTarget {
    pub(crate) environment: EnvironmentName,
    pub(crate) group: GroupId,
    pub(crate) root: PathBuf,
    pub(crate) link: HostLink,
}

#[derive(Debug)]
pub(crate) struct Observed {
    environment: EnvironmentName,
    group: GroupId,
    project: ProjectKey,
    snapshot: CodeHostSnapshot,
}

#[derive(Debug, Default)]
pub(crate) struct CodeHostWatch {
    snapshots: HashMap<ProjectKey, CodeHostSnapshot>,
    next: Option<Instant>,
    observing: bool,
    pub(crate) asked: bool,
}

pub(crate) struct Heard {
    pub(crate) environment: EnvironmentName,
    pub(crate) group: GroupId,
    pub(crate) host: CodeHost,
    pub(crate) event: AppEvent,
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
pub(crate) fn observe(targets: Vec<WatchTarget>, gitlab: Option<&GitlabConfig>) -> Vec<Observed> {
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

impl CodeHostWatch {
    pub(crate) fn due(&mut self, now: Instant, wanted: bool) -> bool {
        if !wanted || self.observing {
            return false;
        }
        let next = *self.next.get_or_insert(now + CODE_HOST_POLL);
        if now < next {
            return false;
        }
        self.next = Some(now + CODE_HOST_POLL);
        self.observing = true;
        true
    }

    pub(crate) fn not_observed(&mut self) {
        self.observing = false;
    }

    pub(crate) fn observed(&mut self, observed: Vec<Observed>) -> Vec<Heard> {
        self.observing = false;
        let mut heard = Vec::new();
        for seen in observed {
            let before = self.snapshots.get(&seen.project);
            let events = code_host_events(before, &seen.snapshot);
            let kept = seen.snapshot.carried_over(before);
            let host = seen.project.0;
            self.snapshots.insert(seen.project, kept);
            heard.extend(events.into_iter().map(|event| Heard {
                environment: seen.environment.clone(),
                group: seen.group,
                host,
                event,
            }));
        }
        heard
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_watch_waits_a_full_period_and_never_overlaps_its_reads() {
        let mut watch = CodeHostWatch::default();
        let start = Instant::now();
        assert!(!watch.due(start, true));
        assert!(!watch.due(start + CODE_HOST_POLL, false));
        assert!(watch.due(start + CODE_HOST_POLL, true));
        assert!(!watch.due(start + CODE_HOST_POLL * 3, true));
        watch.not_observed();
        assert!(watch.due(start + CODE_HOST_POLL * 3, true));
    }
}
