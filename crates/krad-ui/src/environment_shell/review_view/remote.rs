mod answers;

use gpui_kit::{AppContext as _, Context, Window};
use krad_domain::review::{DiffRefs, DiscussionId, MergeOptions, MergeRequest, MergeRequestIid};
use krad_host::HostLink;
use krad_protocol::{Command, Reply, RequestId, RequestRef, ReviewAct, ReviewCommand, ReviewWrite};
use krad_repo::diff_between;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::detail::RequestDetail;
use super::diff::DiffSource;
use super::{Connection, Remote, RequestList};
use crate::environment_shell::EnvironmentShell;

pub(in crate::environment_shell) use answers::refused_answer;

const ROW_FACTS_LIMIT: usize = 50;

impl EnvironmentShell {
    pub(in crate::environment_shell) fn post_review(
        &mut self,
        root: PathBuf,
        act: ReviewAct,
        cx: &mut Context<Self>,
    ) -> RequestId {
        let request = self.runtime.handle.next_request();
        let environment = self.active.clone();
        let command = Command::Review(ReviewCommand {
            environment: environment.clone(),
            root: root.clone(),
            act: act.clone(),
        });
        let pending = self.runtime.handle.send(request, command);
        cx.spawn(async move |this, cx| {
            let Ok(Reply::Refused { message, .. }) = pending.await else {
                return;
            };
            this.update(cx, |shell, cx| {
                if let Some(answer) = refused_answer(&act, message) {
                    shell.review_answered(request, (environment, root), answer, cx);
                } else {
                    shell.review.forget_login();
                    cx.notify();
                }
            })
            .unwrap_or_default();
        })
        .detach();
        request
    }

    pub(super) fn connect_host(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        self.review.connection = Connection::Connecting;
        self.post_review(root, ReviewAct::Connect, cx);
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
        let request = self.post_review(root.to_path_buf(), ReviewAct::List { query }, cx);
        self.review.list = Some(RequestList {
            root: root.to_path_buf(),
            filter,
            request,
            requests: Remote::Loading,
            facts: HashMap::new(),
        });
    }

    fn load_row_facts(&mut self, cx: &mut Context<Self>) {
        let Some(RequestList {
            root,
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
        let act = ReviewAct::Facts {
            project: project.clone(),
            iids,
        };
        let root = root.clone();
        let request = self.post_review(root, act, cx);
        if let Some(list) = self.review.list.as_mut() {
            list.request = request;
        }
    }

    pub(super) fn open_merge_request(
        &mut self,
        project: krad_domain::review::ProjectPath,
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
        if !matches!(self.review.connection, Connection::Ready(_)) {
            return;
        }
        let Some((root, request)) = self.open_target() else {
            return;
        };
        self.post_review(root, ReviewAct::Detail { request }, cx);
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
        let link = self.active_link();
        let read = cx.background_spawn(async move { local_diffs(&link, &root, &refs, &paths) });
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
                if let Some(detail) = shell.review.detail.as_mut() {
                    detail.forget_colors();
                    detail.sync_rows();
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn comment_on_request(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(body) = self
            .review
            .detail
            .as_ref()
            .and_then(|detail| detail.take_draft(window, cx))
        else {
            return;
        };
        self.write_review(ReviewWrite::Comment { body }, cx);
    }

    pub(super) fn send_reply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((discussion, body)) = self
            .review
            .detail
            .as_mut()
            .and_then(|detail| detail.take_reply(window, cx))
        else {
            return;
        };
        self.write_review(ReviewWrite::Reply { discussion, body }, cx);
    }

    pub(super) fn resolve_thread(&mut self, discussion: DiscussionId, cx: &mut Context<Self>) {
        self.write_review(ReviewWrite::Resolve { discussion }, cx);
    }

    pub(super) fn set_approval(&mut self, approve: Approval, cx: &mut Context<Self>) {
        let write = match approve {
            Approval::Approve => ReviewWrite::Approve,
            Approval::Revoke => ReviewWrite::Revoke,
        };
        self.write_review(write, cx);
    }

    pub(super) fn merge_request(&mut self, cx: &mut Context<Self>) {
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
        self.write_review(ReviewWrite::Merge { options }, cx);
    }

    fn write_review(&mut self, write: ReviewWrite, cx: &mut Context<Self>) {
        let Some((root, request)) = self.open_target() else {
            return;
        };
        self.post_review(root, ReviewAct::Write { request, write }, cx);
    }

    pub(super) fn open_target(&self) -> Option<(PathBuf, RequestRef)> {
        let detail = self.review.detail.as_ref()?;
        let request = RequestRef {
            project: detail.project.clone(),
            iid: detail.request.iid,
        };
        Some((detail.root.clone(), request))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Approval {
    Approve,
    Revoke,
}

fn local_diffs(
    link: &HostLink,
    root: &Path,
    refs: &DiffRefs,
    paths: &[(String, String)],
) -> Option<Vec<String>> {
    paths
        .iter()
        .map(|(old, new)| {
            let paths: Vec<&str> = if old == new {
                vec![new.as_str()]
            } else {
                vec![old.as_str(), new.as_str()]
            };
            diff_between(link, root, &refs.base_sha.0, &refs.head_sha.0, &paths).ok()
        })
        .collect()
}
