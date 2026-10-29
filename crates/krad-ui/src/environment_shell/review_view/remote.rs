use gpui_kit::{AppContext as _, Context, Task, Window};
use krad_domain::review::{
    project_on_host, DiffRefs, DiscussionId, MergeOptions, MergeRequest, MergeRequestIid,
    MergeRequestQuery, ProjectPath,
};
use krad_domain::{CodeHost, GitlabSettings};
use krad_gitlab::GitlabConfig;
use krad_repo::{diff_between, remote_url, ORIGIN_REMOTE};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::detail::RequestDetail;
use super::diff::{ChangedFile, DiffSource};
use super::host::{origin_repo, HostClient, HostError};
use super::summary::approval_tally;
use super::Remote;
use super::{Account, Connection, RequestList, RowFacts};
use crate::environment_shell::EnvironmentShell;

const GITLAB_NOT_SIGNED_IN: &str =
    "Sign in on the GitLab page of Settings to review merge requests.";
const GITHUB_NOT_SIGNED_IN: &str =
    "Sign in on the GitHub page of Settings to review pull requests.";
const NOT_A_PROJECT: &str =
    "The group's origin remote does not point at a GitLab or GitHub project";
const ROW_FACTS_LIMIT: usize = 50;

impl EnvironmentShell {
    pub(super) fn connect_host(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        self.review.connection = Connection::Connecting;
        let settings = self.settings.gitlab.clone();
        let sign_in = cx.background_spawn(async move { sign_in(&root, &settings) });
        cx.spawn(async move |this, cx| {
            let connection = sign_in.await;
            this.update(cx, |shell, cx| {
                shell.review.connection = connection;
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn refresh_review(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.review.connection, Connection::Ready(_)) {
            self.review.forget_login();
            cx.notify();
            return;
        }
        self.review.list = None;
        self.load_request_detail(cx);
        cx.notify();
    }

    pub(super) fn ensure_review_list(&mut self, root: &Path, cx: &mut Context<Self>) {
        let filter = self.review.filter;
        if self
            .review
            .list
            .as_ref()
            .is_some_and(|list| list.root == root && list.filter == filter)
        {
            return;
        }
        let Some(client) = self.host_client() else {
            return;
        };
        let Connection::Ready(account) = &self.review.connection else {
            return;
        };
        let query = filter.query(account.me.id);
        if self
            .review
            .list
            .as_ref()
            .is_some_and(|list| list.root != root)
        {
            self.review.detail = None;
        }
        let root = root.to_path_buf();
        self.review.list = Some(RequestList {
            root: root.clone(),
            filter,
            requests: Remote::Loading,
            facts: HashMap::new(),
        });
        let repo = root.clone();
        let load = cx.background_spawn(async move { list_requests(&client, &repo, query) });
        cx.spawn(async move |this, cx| {
            let requests = load.await;
            this.update(cx, |shell, cx| {
                let Some(list) = shell
                    .review
                    .list
                    .as_mut()
                    .filter(|list| list.root == root && list.filter == filter)
                else {
                    return;
                };
                list.requests = requests.map_or_else(Remote::Failed, Remote::Ready);
                shell.load_row_facts(&root, filter, cx);
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn load_row_facts(&mut self, root: &Path, filter: super::ReviewFilter, cx: &mut Context<Self>) {
        let Some(client) = self.host_client() else {
            return;
        };
        let Some(RequestList {
            requests: Remote::Ready((project, requests)),
            ..
        }) = self.review.list.as_ref()
        else {
            return;
        };
        let iids: Vec<MergeRequestIid> = requests
            .iter()
            .take(ROW_FACTS_LIMIT)
            .map(|request| request.iid)
            .collect();
        for iid in iids {
            let facts = fetch(cx, &client, project, move |gitlab, project| {
                row_facts(gitlab, project, iid)
            });
            let root = root.to_path_buf();
            cx.spawn(async move |this, cx| {
                let Remote::Ready(facts) = facts.await else {
                    return;
                };
                this.update(cx, |shell, cx| {
                    let Some(list) = shell
                        .review
                        .list
                        .as_mut()
                        .filter(|list| list.root == root && list.filter == filter)
                    else {
                        return;
                    };
                    list.facts.insert(iid, facts);
                    cx.notify();
                })
                .unwrap_or_default();
            })
            .detach();
        }
    }

    pub(super) fn open_merge_request(
        &mut self,
        project: ProjectPath,
        iid: MergeRequestIid,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(request) = self.listed_request(iid).cloned() else {
            return;
        };
        let Some(root) = self.review.list.as_ref().map(|list| list.root.clone()) else {
            return;
        };
        let words = self.review_words();
        self.review.detail = Some(RequestDetail::new(
            root, project, request, words, window, cx,
        ));
        self.load_request_detail(cx);
        cx.notify();
    }

    fn listed_request(&self, iid: MergeRequestIid) -> Option<&MergeRequest> {
        let Remote::Ready((_, requests)) = &self.review.list.as_ref()?.requests else {
            return None;
        };
        requests.iter().find(|request| request.iid == iid)
    }

    pub(super) fn load_request_detail(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.host_client() else {
            return;
        };
        let Some(detail) = &self.review.detail else {
            return;
        };
        let iid = detail.request.iid;
        let project = detail.project.clone();
        let request = fetch(cx, &client, &project, move |gitlab, project| {
            let request = gitlab.merge_request(project, iid)?;
            let jobs = request
                .head_pipeline
                .as_ref()
                .map(|pipeline| gitlab.jobs(project, pipeline.id));
            Ok((request, jobs))
        });
        let commits = fetch(cx, &client, &project, move |gitlab, project| {
            gitlab.commits(project, iid)
        });
        let files = fetch(cx, &client, &project, move |gitlab, project| {
            gitlab
                .changes(project, iid)
                .map(|changes| changes.into_iter().map(ChangedFile::new).collect())
        });
        let discussions = fetch(cx, &client, &project, move |gitlab, project| {
            gitlab.discussions(project, iid)
        });
        let approvals = fetch(cx, &client, &project, move |gitlab, project| {
            gitlab.approvals(project, iid)
        });
        let pipelines = fetch(cx, &client, &project, move |gitlab, project| {
            gitlab.pipelines(project, iid)
        });
        cx.spawn(async move |this, cx| {
            let (request, commits, files, discussions, approvals, pipelines) =
                futures::join!(request, commits, files, discussions, approvals, pipelines);
            this.update(cx, |shell, cx| {
                let Some(detail) = shell
                    .review
                    .detail
                    .as_mut()
                    .filter(|detail| detail.request.iid == iid)
                else {
                    return;
                };
                detail.jobs = match request {
                    Remote::Ready((request, jobs)) => {
                        detail.request = request;
                        jobs.map_or(Remote::Ready(Vec::new()), |jobs| {
                            jobs.map_or_else(
                                |error| Remote::Failed(error.to_string()),
                                Remote::Ready,
                            )
                        })
                    }
                    Remote::Failed(reason) => Remote::Failed(reason),
                    Remote::Loading => Remote::Loading,
                };
                detail.commits = commits;
                detail.files = files;
                detail.discussions = discussions;
                detail.approvals = approvals;
                detail.pipelines = pipelines;
                shell.read_local_diffs(cx);
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn read_local_diffs(&mut self, cx: &mut Context<Self>) {
        let Some(detail) = &self.review.detail else {
            return;
        };
        let (Some(refs), Remote::Ready(files)) = (&detail.request.diff_refs, &detail.files) else {
            return;
        };
        let iid = detail.request.iid;
        let root = detail.root.clone();
        let refs = refs.clone();
        let paths: Vec<(String, String)> = files
            .iter()
            .map(|file| (file.change.old_path.clone(), file.change.new_path.clone()))
            .collect();
        let read = cx.background_spawn(async move { local_diffs(&root, &refs, &paths) });
        cx.spawn(async move |this, cx| {
            let Some(diffs) = read.await else {
                return;
            };
            this.update(cx, |shell, cx| {
                let Some(RequestDetail {
                    files: Remote::Ready(files),
                    ..
                }) = shell
                    .review
                    .detail
                    .as_mut()
                    .filter(|detail| detail.request.iid == iid)
                else {
                    return;
                };
                for (file, diff) in files.iter_mut().zip(&diffs) {
                    file.set_diff(diff, DiffSource::LocalCheckout);
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn comment_on_request(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((client, project, iid)) = self.open_target() else {
            return;
        };
        let Some(body) = self
            .review
            .detail
            .as_ref()
            .and_then(|detail| detail.take_draft(window, cx))
        else {
            return;
        };
        let added =
            cx.background_spawn(async move { client.add_note(&project, iid, &body).map(drop) });
        Self::reload_after_write(added, cx);
    }

    pub(super) fn send_reply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((client, project, iid)) = self.open_target() else {
            return;
        };
        let Some((discussion, body)) = self
            .review
            .detail
            .as_mut()
            .and_then(|detail| detail.take_reply(window, cx))
        else {
            return;
        };
        let replied = cx.background_spawn(async move {
            client.reply(&project, iid, &discussion, &body).map(drop)
        });
        Self::reload_after_write(replied, cx);
    }

    pub(super) fn resolve_thread(&mut self, discussion: DiscussionId, cx: &mut Context<Self>) {
        let Some((client, project, iid)) = self.open_target() else {
            return;
        };
        let resolved = cx.background_spawn(async move {
            client.resolve_discussion(&project, iid, &discussion, true)
        });
        Self::reload_after_write(resolved, cx);
    }

    pub(super) fn set_approval(&mut self, approve: Approval, cx: &mut Context<Self>) {
        let Some((client, project, iid)) = self.open_target() else {
            return;
        };
        let changed = cx.background_spawn(async move {
            match approve {
                Approval::Approve => client.approve(&project, iid),
                Approval::Revoke => client.unapprove(&project, iid),
            }
        });
        Self::reload_after_write(changed, cx);
    }

    pub(super) fn merge_request(&mut self, cx: &mut Context<Self>) {
        let Some((client, project, iid)) = self.open_target() else {
            return;
        };
        let Some(detail) = self.review.detail.as_mut() else {
            return;
        };
        if !detail.confirm_merge {
            detail.confirm_merge = true;
            cx.notify();
            return;
        }
        detail.confirm_merge = false;
        let options = MergeOptions {
            sha: detail.request.sha.clone(),
            squash: detail.squash,
            should_remove_source_branch: detail.remove_source_branch,
        };
        let merged = cx.background_spawn(async move { client.merge(&project, iid, &options) });
        cx.spawn(async move |this, cx| {
            let merged = merged.await;
            this.update(cx, |shell, cx| {
                if let Err(error) = merged {
                    shell.toast(&error.to_string(), cx);
                }
                shell.review.list = None;
                shell.load_request_detail(cx);
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn reload_after_write(change: Task<Result<(), HostError>>, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let result = change.await;
            this.update(cx, |shell, cx| {
                if let Err(error) = result {
                    shell.toast(&error.to_string(), cx);
                }
                shell.load_request_detail(cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn host_client(&self) -> Option<Arc<HostClient>> {
        match &self.review.connection {
            Connection::Ready(account) => Some(Arc::clone(&account.client)),
            _ => None,
        }
    }

    fn open_target(&self) -> Option<(Arc<HostClient>, ProjectPath, MergeRequestIid)> {
        let detail = self.review.detail.as_ref()?;
        Some((
            self.host_client()?,
            detail.project.clone(),
            detail.request.iid,
        ))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Approval {
    Approve,
    Revoke,
}

fn fetch<T: Send + 'static>(
    cx: &mut Context<EnvironmentShell>,
    client: &Arc<HostClient>,
    project: &ProjectPath,
    call: impl FnOnce(&HostClient, &ProjectPath) -> Result<T, HostError> + Send + 'static,
) -> Task<Remote<T>> {
    let client = Arc::clone(client);
    let project = project.clone();
    cx.background_spawn(async move {
        call(&client, &project)
            .map_or_else(|error| Remote::Failed(error.to_string()), Remote::Ready)
    })
}

fn sign_in(root: &Path, gitlab: &GitlabSettings) -> Connection {
    let Some(repo) = origin_repo(root, GitlabConfig::try_from(gitlab).ok()) else {
        return Connection::SignedOut(NOT_A_PROJECT.to_owned());
    };
    let client = match repo.login.client() {
        Ok(Some(client)) => client,
        Ok(None) => {
            let hint = match repo.login.code_host() {
                CodeHost::Gitlab => GITLAB_NOT_SIGNED_IN,
                CodeHost::Github => GITHUB_NOT_SIGNED_IN,
            };
            return Connection::SignedOut(hint.to_owned());
        }
        Err(error) => return Connection::Failed(error.to_string()),
    };
    match client.current_user() {
        Ok(me) => Connection::Ready(Account {
            client: Arc::new(client),
            me,
        }),
        Err(error) if error.is_signed_out() => Connection::SignedOut(error.to_string()),
        Err(error) => Connection::Failed(error.to_string()),
    }
}

fn list_requests(
    client: &HostClient,
    repo: &Path,
    query: MergeRequestQuery,
) -> Result<(ProjectPath, Vec<MergeRequest>), String> {
    let url = remote_url(repo, ORIGIN_REMOTE).map_err(|error| error.to_string())?;
    let project = url
        .and_then(|url| project_on_host(&url, client.host()))
        .ok_or_else(|| NOT_A_PROJECT.to_owned())?;
    let requests = client
        .merge_requests(&project, query)
        .map_err(|error| error.to_string())?;
    Ok((project, requests))
}

fn row_facts(
    client: &HostClient,
    project: &ProjectPath,
    iid: MergeRequestIid,
) -> Result<RowFacts, HostError> {
    let request = client.merge_request(project, iid)?;
    let approvals = client.approvals(project, iid)?;
    Ok(RowFacts {
        pipeline: request.head_pipeline.map(|pipeline| pipeline.status),
        approvals: approval_tally(&approvals),
    })
}

fn local_diffs(root: &Path, refs: &DiffRefs, paths: &[(String, String)]) -> Option<Vec<String>> {
    paths
        .iter()
        .map(|(old, new)| {
            let paths: Vec<&str> = if old == new {
                vec![new.as_str()]
            } else {
                vec![old.as_str(), new.as_str()]
            };
            diff_between(root, &refs.base_sha.0, &refs.head_sha.0, &paths).ok()
        })
        .collect()
}
