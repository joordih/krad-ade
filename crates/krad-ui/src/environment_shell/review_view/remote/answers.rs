use gpui_kit::Context;
use krad_domain::{EnvironmentName, ToastKind};
use krad_protocol::{
    Fetched, ListedRequests, RequestDetailRead, RequestId, ReviewAct, ReviewAnswer,
    ReviewConnection, RowFacts,
};
use std::path::PathBuf;

use super::super::diff::ChangedFile;
use super::super::summary::approval_tally;
use super::super::{Account, Connection, Remote, RowFacts as ShownFacts};
use crate::environment_shell::EnvironmentShell;

impl<T> From<Fetched<T>> for Remote<T> {
    fn from(fetched: Fetched<T>) -> Self {
        match fetched {
            Fetched::Ready(value) => Self::Ready(value),
            Fetched::Failed(reason) => Self::Failed(reason),
        }
    }
}

impl From<&RowFacts> for ShownFacts {
    fn from(facts: &RowFacts) -> Self {
        Self {
            pipeline: facts.pipeline,
            approvals: approval_tally(&facts.approvals),
        }
    }
}

pub(in crate::environment_shell) fn refused_answer(
    act: &ReviewAct,
    message: String,
) -> Option<ReviewAnswer> {
    match act {
        ReviewAct::Probe => Some(ReviewAnswer::Probed { host: None }),
        ReviewAct::Connect => Some(ReviewAnswer::Connected {
            connection: ReviewConnection::Failed { message },
        }),
        ReviewAct::Issues => Some(ReviewAnswer::Issues {
            issues: Fetched::Failed(message),
        }),
        ReviewAct::List { .. }
        | ReviewAct::Facts { .. }
        | ReviewAct::Detail { .. }
        | ReviewAct::Write { .. }
        | ReviewAct::FixJob { .. }
        | ReviewAct::Unknown => None,
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn review_answered(
        &mut self,
        request: RequestId,
        (environment, root): (EnvironmentName, PathBuf),
        answer: ReviewAnswer,
        cx: &mut Context<Self>,
    ) {
        match answer {
            ReviewAnswer::Probed { host } => self.review_probed(&environment, root, host),
            ReviewAnswer::Connected { connection } => self.review_connected(connection),
            ReviewAnswer::Listed { listed } => self.requests_listed(request, &root, listed, cx),
            ReviewAnswer::Facts { iid, facts } => {
                let Some(list) = self
                    .review
                    .list
                    .as_mut()
                    .filter(|list| list.request == request)
                else {
                    return;
                };
                if let Fetched::Ready(facts) = &facts {
                    list.facts.insert(iid, facts.into());
                }
            }
            ReviewAnswer::Detail { detail } => self.detail_read(*detail, cx),
            ReviewAnswer::Issues { issues } => self.issues_answered(&root, issues.into()),
            ReviewAnswer::Written { failure } => {
                if let Some(failure) = failure {
                    self.toast(ToastKind::Error, &failure, cx);
                }
                self.review.list = None;
                self.load_request_detail(cx);
            }
            ReviewAnswer::Fixed { job, fix } => self.fix_answered(job, fix, cx),
            ReviewAnswer::Unknown => {}
        }
        cx.notify();
    }

    fn review_connected(&mut self, connection: ReviewConnection) {
        if !matches!(self.review.connection, Connection::Connecting) {
            return;
        }
        self.review.connection = match connection {
            ReviewConnection::SignedOut { reason } => Connection::SignedOut(reason),
            ReviewConnection::Failed { message } => Connection::Failed(message),
            ReviewConnection::Ready { host, me } => Connection::Ready(Account { host, me }),
        };
    }

    fn requests_listed(
        &mut self,
        request: RequestId,
        root: &PathBuf,
        listed: Fetched<ListedRequests>,
        cx: &mut Context<Self>,
    ) {
        let Some(list) = self
            .review
            .list
            .as_mut()
            .filter(|list| list.request == request && &list.root == root)
        else {
            return;
        };
        list.requests = match listed {
            Fetched::Ready(ListedRequests { project, requests }) => {
                Remote::Ready((project, requests))
            }
            Fetched::Failed(reason) => Remote::Failed(reason),
        };
        self.load_row_facts(cx);
    }

    fn detail_read(&mut self, read: RequestDetailRead, cx: &mut Context<Self>) {
        let Some(detail) = self
            .review
            .detail
            .as_mut()
            .filter(|detail| detail.request.iid == read.iid)
        else {
            return;
        };
        detail.jobs = match read.request {
            Fetched::Ready(request) => {
                detail.request = request;
                read.jobs.into()
            }
            Fetched::Failed(reason) => Remote::Failed(reason),
        };
        detail.commits = read.commits.into();
        detail.files = Remote::from(read.changes)
            .map(|changes| changes.into_iter().map(ChangedFile::new).collect());
        detail.discussions = read.discussions.into();
        detail.approvals = read.approvals.into();
        detail.pipelines = read.pipelines.into();
        self.show_review_file(cx);
        self.read_local_diffs(cx);
    }
}

impl<T> Remote<T> {
    fn map<U>(self, change: impl FnOnce(T) -> U) -> Remote<U> {
        match self {
            Self::Loading => Remote::Loading,
            Self::Failed(reason) => Remote::Failed(reason),
            Self::Ready(value) => Remote::Ready(change(value)),
        }
    }
}
