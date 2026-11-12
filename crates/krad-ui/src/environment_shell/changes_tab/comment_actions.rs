use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::{AppContext as _, Context, Entity, SharedString, Subscription, Window};
use krad_domain::{
    review_message, DiffAnchor, DiffComment, DiffCommentId, EnvironmentName, HunkLines, Session,
    TerminalId, ToastKind,
};

use super::super::{EnvironmentShell, LiveEnv};
use super::comment_view::comment_title;
use super::comments::{quote, reanchor, relocate, same_hunk, PlacedComment};
use super::{DiffLoad, FileDiff};
use crate::keymap::COMMIT_HINT;

const DRAFT_MAX_ROWS: usize = 8;
const REVIEW_SENT: &str = "Review sent to the session's agent";
const NO_AGENT: &str = "This session has no chat to send the review to";
const DRAFT_MOVED: &str = "The lines this comment was on changed, so it was not saved";

pub(super) enum DraftTarget {
    New {
        one: usize,
        other: usize,
        anchor: Option<DiffAnchor>,
    },
    Edit(DiffCommentId),
}

impl FileDiff {
    pub(super) fn relocate_draft(&mut self) {
        let DiffLoad::Ready(read) = &self.load else {
            return;
        };
        let Some(CommentDraft {
            target:
                DraftTarget::New {
                    one,
                    other,
                    anchor: Some(anchor),
                },
            ..
        }) = &mut self.draft
        else {
            return;
        };
        if let Some((first, last)) = relocate(&read.lines, anchor) {
            (*one, *other) = (first, last);
        }
    }
}

pub(super) struct CommentDraft {
    pub(super) target: DraftTarget,
    pub(super) field: Entity<TextareaState>,
    pub(super) title: Option<SharedString>,
    _events: Subscription,
}

impl CommentDraft {
    pub(super) fn covers(&self, index: usize) -> bool {
        match self.target {
            DraftTarget::New { one, other, .. } => {
                HunkLines::new(one, other).range().contains(&index)
            }
            DraftTarget::Edit(_) => false,
        }
    }
}

fn draft_placeholder() -> String {
    format!("Comment for the agent ({COMMIT_HINT} to save)")
}

impl LiveEnv {
    pub(super) fn tab_session(&self, tab: TerminalId) -> Option<&Session> {
        self.data.session(self.data.session_of(tab)?)
    }

    fn tab_session_mut(&mut self, tab: TerminalId) -> Option<&mut Session> {
        let id = self.data.session_of(tab)?;
        self.data
            .sessions
            .iter_mut()
            .find(|session| session.id == id)
    }

    pub(super) fn tab_comment(&self, tab: TerminalId, id: DiffCommentId) -> Option<&DiffComment> {
        self.tab_session(tab)?
            .diff_comments
            .iter()
            .find(|comment| comment.id == id)
    }

    pub(super) fn tab_review_size(&self, tab: TerminalId) -> usize {
        self.tab_session(tab)
            .map_or(0, |session| session.diff_comments.len())
    }

    fn sync_diff_comments(&mut self, tab: TerminalId) -> bool {
        let id = self.data.session_of(tab);
        let Some(diff) = self.diffs.get_mut(&tab) else {
            return false;
        };
        let session = id.and_then(|id| self.data.sessions.iter_mut().find(|s| s.id == id));
        let (DiffLoad::Ready(read), Some(session)) = (&diff.load, session) else {
            diff.comments.clear();
            diff.refold();
            return false;
        };
        let mut changed = false;
        diff.comments = Vec::new();
        for comment in &mut session.diff_comments {
            if comment.quote.anchor.path != diff.path {
                continue;
            }
            changed |= reanchor(&read.lines, std::slice::from_mut(comment));
            diff.comments.push(PlacedComment::new(&read.lines, comment));
        }
        diff.refold();
        changed
    }
}

impl EnvironmentShell {
    pub(super) fn refresh_diff_comments(&mut self, env: &EnvironmentName, tab: TerminalId) {
        let changed = self
            .envs
            .get_mut(env)
            .is_some_and(|live| live.sync_diff_comments(tab));
        if changed {
            self.store();
        }
    }

    fn open_draft(
        &mut self,
        tab: TerminalId,
        target: DraftTarget,
        title: Option<SharedString>,
        initial: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let field = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, DRAFT_MAX_ROWS)
                .placeholder(draft_placeholder())
                .default_value(initial)
        });
        let focused = field.clone();
        cx.defer_in(window, move |_, window, cx| {
            focused.update(cx, |field, cx| field.focus(window, cx));
        });
        let events = cx.subscribe_in(
            &field,
            window,
            move |this: &mut Self, _, event: &InputEvent, _, cx| {
                if let InputEvent::PressEnter {
                    secondary: true, ..
                } = event
                {
                    this.save_comment(tab, cx);
                }
            },
        );
        let env = self.active.clone();
        if let Some(diff) = self
            .envs
            .get_mut(&env)
            .and_then(|live| live.diffs.get_mut(&tab))
        {
            diff.draft = Some(CommentDraft {
                target,
                field,
                title,
                _events: events,
            });
        }
        cx.notify();
    }

    pub(super) fn start_comment(
        &mut self,
        tab: TerminalId,
        line: usize,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        let Some(diff) = self
            .envs
            .get_mut(&env)
            .and_then(|live| live.diffs.get_mut(&tab))
        else {
            return;
        };
        let DiffLoad::Ready(read) = &diff.load else {
            return;
        };
        if let Some(CommentDraft {
            target: DraftTarget::New { one, other, anchor },
            title,
            ..
        }) = &mut diff.draft
        {
            if extend && same_hunk(&read.lines, *one, line) {
                *other = line;
                let quoted = quote(&read.lines, &diff.path, *one, line);
                *title = quoted.as_ref().map(|quoted| comment_title(quoted.span));
                *anchor = quoted.map(|quoted| quoted.anchor);
                cx.notify();
                return;
            }
        }
        let quoted = quote(&read.lines, &diff.path, line, line);
        let title = quoted.as_ref().map(|quoted| comment_title(quoted.span));
        self.open_draft(
            tab,
            DraftTarget::New {
                one: line,
                other: line,
                anchor: quoted.map(|quoted| quoted.anchor),
            },
            title,
            String::new(),
            window,
            cx,
        );
    }

    pub(super) fn edit_comment(
        &mut self,
        tab: TerminalId,
        id: DiffCommentId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((title, text)) = self
            .envs
            .get(&self.active)
            .and_then(|live| live.tab_comment(tab, id))
            .map(|comment| (comment_title(comment.quote.span), comment.text.clone()))
        else {
            return;
        };
        self.open_draft(tab, DraftTarget::Edit(id), Some(title), text, window, cx);
    }

    pub(super) fn cancel_comment(&mut self, tab: TerminalId, cx: &mut Context<Self>) {
        let env = self.active.clone();
        if let Some(diff) = self
            .envs
            .get_mut(&env)
            .and_then(|live| live.diffs.get_mut(&tab))
        {
            diff.draft = None;
        }
        cx.notify();
    }

    pub(super) fn save_comment(&mut self, tab: TerminalId, cx: &mut Context<Self>) {
        let env = self.active.clone();
        let Some(live) = self.envs.get_mut(&env) else {
            return;
        };
        let Some(diff) = live.diffs.get_mut(&tab) else {
            return;
        };
        let Some(draft) = diff.draft.take() else {
            return;
        };
        let text = draft.field.read(cx).value().trim().to_owned();
        let quoted = match (&diff.load, &draft.target) {
            (
                DiffLoad::Ready(read),
                DraftTarget::New {
                    anchor: Some(anchor),
                    ..
                },
            ) => relocate(&read.lines, anchor)
                .and_then(|(one, other)| quote(&read.lines, &diff.path, one, other)),
            _ => None,
        };
        let mut moved = false;
        if let Some(session) = live.tab_session_mut(tab) {
            match draft.target {
                DraftTarget::New { .. } if text.is_empty() => {}
                DraftTarget::New { .. } => match quoted {
                    Some(quoted) => {
                        session.add_diff_comment(quoted, text);
                    }
                    None => moved = true,
                },
                DraftTarget::Edit(id) if text.is_empty() => {
                    session.remove_diff_comment(id);
                }
                DraftTarget::Edit(id) => {
                    session.edit_diff_comment(id, text);
                }
            }
        }
        if moved {
            self.toast(ToastKind::Warning, DRAFT_MOVED, cx);
        }
        self.refresh_diff_comments(&env, tab);
        self.store();
        cx.notify();
    }

    pub(super) fn delete_comment(
        &mut self,
        tab: TerminalId,
        id: DiffCommentId,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        let removed = self
            .envs
            .get_mut(&env)
            .and_then(|live| live.tab_session_mut(tab))
            .is_some_and(|session| session.remove_diff_comment(id));
        if !removed {
            return;
        }
        self.refresh_diff_comments(&env, tab);
        self.store();
        cx.notify();
    }

    pub(super) fn send_review(&mut self, tab: TerminalId, cx: &mut Context<Self>) {
        let env = self.active.clone();
        let Some(live) = self.envs.get(&env) else {
            return;
        };
        let Some(session) = live.tab_session(tab) else {
            return;
        };
        if session.diff_comments.is_empty() {
            return;
        }
        let root = session
            .group
            .and_then(|group| live.data.group(group))
            .map_or_default(|group| group.root.clone());
        let message = review_message(&root, &session.diff_comments);
        let Some(recipient) = live.data.review_recipient(session.id) else {
            self.toast(ToastKind::Warning, NO_AGENT, cx);
            return;
        };
        if !self.queue_chat_prompt(&env, recipient, &message, cx) {
            self.toast(ToastKind::Warning, NO_AGENT, cx);
            return;
        }
        if let Some(session) = self
            .envs
            .get_mut(&env)
            .and_then(|live| live.tab_session_mut(tab))
        {
            session.diff_comments.clear();
        }
        self.refresh_diff_comments(&env, tab);
        self.store();
        self.toast(ToastKind::Success, REVIEW_SENT, cx);
    }
}
