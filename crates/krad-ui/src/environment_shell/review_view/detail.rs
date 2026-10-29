use gpui_kit::component::input::TextareaState;
use gpui_kit::{
    div, px, AnyElement, AppContext as _, Context, Entity, InteractiveElement, IntoElement,
    ParentElement, StatefulInteractiveElement, Styled, Window,
};
use krad_domain::review::{
    Approvals, Commit, Discussion, DiscussionId, Job, MergeRequest, MergeRequestState, MergeStatus,
    Pipeline, ProjectPath, UserId,
};
use std::collections::HashSet;
use std::path::PathBuf;

use super::diff::ChangedFile;
use super::remote::Approval;
use super::threads::ReplyDraft;
use super::words::HostWords;
use super::Remote;
use crate::environment_shell::EnvironmentShell;
use crate::theme::palette;

pub(super) const HEADER_PAD_X: f32 = 16.0;
const SCROLL_ID: &str = "review-detail";
const MERGE_LABEL: &str = "Merge";
const CONFIRM_MERGE_LABEL: &str = "Click again to merge";
const COMMENT_ROWS: usize = 2;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(super) enum ReviewTab {
    #[default]
    Overview,
    Commits,
    Changes,
    Pipelines,
}

impl ReviewTab {
    pub(super) const ALL: [Self; 4] = [
        Self::Overview,
        Self::Commits,
        Self::Changes,
        Self::Pipelines,
    ];

    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Commits => "Commits",
            Self::Changes => "Changes",
            Self::Pipelines => "Pipelines",
        }
    }
}

pub(in crate::environment_shell) struct RequestDetail {
    pub(super) root: PathBuf,
    pub(super) project: ProjectPath,
    pub(super) request: MergeRequest,
    pub(super) tab: ReviewTab,
    pub(super) commits: Remote<Vec<Commit>>,
    pub(super) files: Remote<Vec<ChangedFile>>,
    pub(super) selected_file: usize,
    pub(super) discussions: Remote<Vec<Discussion>>,
    pub(super) approvals: Remote<Approvals>,
    pub(super) pipelines: Remote<Vec<Pipeline>>,
    pub(super) jobs: Remote<Vec<Job>>,
    pub(super) draft: Entity<TextareaState>,
    pub(super) reply: Option<ReplyDraft>,
    pub(super) expanded: HashSet<DiscussionId>,
    pub(super) remove_source_branch: bool,
    pub(super) squash: bool,
    pub(super) confirm_merge: bool,
    pub(super) words: &'static HostWords,
}

impl RequestDetail {
    pub(super) fn new(
        root: PathBuf,
        project: ProjectPath,
        request: MergeRequest,
        words: &'static HostWords,
        window: &mut Window,
        cx: &mut Context<EnvironmentShell>,
    ) -> Self {
        let draft = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(COMMENT_ROWS)
                .placeholder(words.draft_placeholder)
        });
        Self {
            root,
            project,
            request,
            words,
            tab: ReviewTab::default(),
            commits: Remote::Loading,
            files: Remote::Loading,
            selected_file: 0,
            discussions: Remote::Loading,
            approvals: Remote::Loading,
            pipelines: Remote::Loading,
            jobs: Remote::Loading,
            draft,
            reply: None,
            expanded: HashSet::new(),
            remove_source_branch: true,
            squash: false,
            confirm_merge: false,
        }
    }

    pub(super) fn take_draft(
        &self,
        window: &mut Window,
        cx: &mut Context<EnvironmentShell>,
    ) -> Option<String> {
        let text = self.draft.read(cx).value().trim().to_owned();
        if text.is_empty() {
            return None;
        }
        self.draft
            .update(cx, |field, cx| field.set_value("", window, cx));
        Some(text)
    }

    pub(super) fn take_reply(
        &mut self,
        window: &mut Window,
        cx: &mut Context<EnvironmentShell>,
    ) -> Option<(DiscussionId, String)> {
        let reply = self.reply.as_ref()?;
        let text = reply.field.read(cx).value().trim().to_owned();
        if text.is_empty() {
            return None;
        }
        reply
            .field
            .update(cx, |field, cx| field.set_value("", window, cx));
        let discussion = reply.discussion.clone();
        self.reply = None;
        Some((discussion, text))
    }

    pub(super) fn approval_action(&self, me: Option<UserId>) -> Option<Approval> {
        let Remote::Ready(approvals) = &self.approvals else {
            return None;
        };
        let mine = approvals
            .approved_by
            .iter()
            .any(|approval| Some(approval.user.id) == me);
        Some(if mine {
            Approval::Revoke
        } else {
            Approval::Approve
        })
    }

    pub(super) fn is_mergeable(&self) -> bool {
        self.request.state == MergeRequestState::Opened
            && self.request.detailed_merge_status == Some(MergeStatus::Mergeable)
    }

    pub(super) fn merge_label(&self) -> &'static str {
        if self.confirm_merge {
            CONFIRM_MERGE_LABEL
        } else {
            MERGE_LABEL
        }
    }

    pub(super) fn count(&self, tab: ReviewTab) -> Option<usize> {
        match tab {
            ReviewTab::Overview => None,
            ReviewTab::Commits => ready_len(&self.commits),
            ReviewTab::Changes => ready_len(&self.files),
            ReviewTab::Pipelines => ready_len(&self.pipelines),
        }
    }
}

fn ready_len<T>(remote: &Remote<Vec<T>>) -> Option<usize> {
    match remote {
        Remote::Ready(items) => Some(items.len()),
        Remote::Loading | Remote::Failed(_) => None,
    }
}

impl EnvironmentShell {
    pub(super) fn request_detail(
        &self,
        detail: &RequestDetail,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let body = match detail.tab {
            ReviewTab::Overview => self.overview(detail, cx),
            ReviewTab::Commits => super::tabs::commits(&detail.commits, pal),
            ReviewTab::Changes => return self.with_header(detail, Self::changes(detail, cx), cx),
            ReviewTab::Pipelines => super::tabs::pipelines(&detail.pipelines, detail.words, pal),
        };
        let scrolled = div()
            .id(SCROLL_ID)
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(px(HEADER_PAD_X))
            .child(body)
            .into_any_element();
        self.with_header(detail, scrolled, cx)
    }
}
