use futures::channel::oneshot::Sender as ReplySender;
use gpui_kit::FocusHandle;
use krad_agents::Response;
use krad_chat::{AskKind, ToolAsk};
use krad_domain::{AskOutcome, AskVerdict, ToolDecision};

use super::{outcome, Note, NotePurpose, Picks};

pub(in crate::environment_shell) enum AskReply {
    Live,
    Hook(ReplySender<Response>),
    Keys,
}

pub(in crate::environment_shell) struct Waiting {
    pub(super) ask: ToolAsk,
    pub(super) reply: AskReply,
    tool: Option<String>,
}

impl Waiting {
    pub(in crate::environment_shell) const fn live(ask: ToolAsk) -> Self {
        Self {
            ask,
            reply: AskReply::Live,
            tool: None,
        }
    }

    pub(in crate::environment_shell) const fn held(
        ask: ToolAsk,
        reply: AskReply,
        tool: String,
    ) -> Self {
        Self {
            ask,
            reply,
            tool: Some(tool),
        }
    }

    pub(super) const fn by_keys(&self) -> bool {
        matches!(self.reply, AskReply::Keys)
    }

    fn let_go(self) {
        if let AskReply::Hook(sender) = self.reply {
            sender
                .send(Response::Decided(ToolDecision::Pass))
                .unwrap_or_default();
        }
    }
}

#[derive(Default)]
pub(in crate::environment_shell) struct AskQueue {
    pub(super) waiting: Vec<Waiting>,
    pub(super) picks: Picks,
    pub(super) note: Option<Note>,
    pub(super) focus: Option<FocusHandle>,
    pub(super) claims_focus: bool,
}

impl AskQueue {
    pub(in crate::environment_shell) const fn is_empty(&self) -> bool {
        self.waiting.is_empty()
    }

    pub(in crate::environment_shell) fn push(&mut self, waiting: Waiting) {
        self.waiting.push(waiting);
        self.claims_focus = true;
    }

    pub(super) fn other_question(&self) -> Option<String> {
        match &self.note.as_ref()?.purpose {
            NotePurpose::OtherAnswer { question } => Some(question.clone()),
            NotePurpose::DenyReason | NotePurpose::PlanFeedback => None,
        }
    }

    pub(super) fn drop_other_answer(&mut self) {
        if self.other_question().is_some() {
            self.note = None;
        }
    }

    pub(in crate::environment_shell) fn blocks_typing(&self) -> bool {
        self.front()
            .is_some_and(|waiting| matches!(waiting.ask.kind, AskKind::Questions(_)))
    }

    pub(super) fn front(&self) -> Option<&Waiting> {
        self.waiting.first()
    }

    pub(super) fn take_front(&mut self) -> Option<Waiting> {
        if self.waiting.is_empty() {
            return None;
        }
        self.picks = Picks::default();
        self.note = None;
        Some(self.waiting.remove(0))
    }

    pub(in crate::environment_shell) fn cancel_all(&mut self) -> Vec<AskOutcome> {
        self.picks = Picks::default();
        self.note = None;
        self.waiting
            .drain(..)
            .map(|waiting| {
                let line = outcome(&waiting.ask.kind, AskVerdict::Cancelled);
                waiting.let_go();
                line
            })
            .collect()
    }

    pub(in crate::environment_shell) fn settle(&mut self, tool: Option<&str>) {
        let settled = |waiting: &Waiting| tool.is_none() || waiting.tool.as_deref() == tool;
        let front_settled = self.waiting.first().is_some_and(settled);
        let (gone, kept): (Vec<Waiting>, Vec<Waiting>) = self.waiting.drain(..).partition(settled);
        self.waiting = kept;
        if front_settled {
            self.picks = Picks::default();
            self.note = None;
        }
        gone.into_iter().for_each(Waiting::let_go);
    }
}
