use futures::channel::oneshot::Sender as ReplySender;
use gpui_kit::FocusHandle;
use krad_agents::Response;
use krad_chat::{AskKind, ToolAsk};
use krad_domain::{AskOutcome, AskVerdict, ToolDecision, ToolUseId};

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
    call: Option<ToolUseId>,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::environment_shell) enum Settled<'a> {
    All,
    OfTool(&'a str),
    PastTool(&'a str),
    Call {
        call: &'a ToolUseId,
        tool: Option<&'a str>,
    },
    Plans,
}

impl Settled<'_> {
    fn covers(self, waiting: &Waiting) -> bool {
        let of = |tool: &str| waiting.tool.as_deref() == Some(tool);
        match self {
            Self::All => true,
            Self::OfTool(tool) => of(tool) || waiting.by_keys(),
            Self::PastTool(tool) => waiting.by_keys() && !of(tool),
            Self::Call { call, tool } => match &waiting.call {
                Some(asked) => asked == call,
                None => tool.is_some_and(of),
            },
            Self::Plans => waiting.by_keys() && matches!(waiting.ask.kind, AskKind::Plan { .. }),
        }
    }
}

impl Waiting {
    pub(in crate::environment_shell) const fn live(ask: ToolAsk) -> Self {
        Self {
            ask,
            reply: AskReply::Live,
            tool: None,
            call: None,
        }
    }

    pub(in crate::environment_shell) const fn held(
        ask: ToolAsk,
        reply: AskReply,
        tool: String,
        call: Option<ToolUseId>,
    ) -> Self {
        Self {
            ask,
            reply,
            tool: Some(tool),
            call,
        }
    }

    pub(super) const fn by_keys(&self) -> bool {
        matches!(self.reply, AskReply::Keys)
    }

    fn caller_left(&self) -> bool {
        matches!(&self.reply, AskReply::Hook(sender) if sender.is_canceled())
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

    pub(in crate::environment_shell) fn settle(&mut self, settled: Settled<'_>) -> bool {
        self.drop_where(|waiting| settled.covers(waiting))
    }

    pub(in crate::environment_shell) fn drop_left_callers(&mut self) -> bool {
        self.drop_where(Waiting::caller_left)
    }

    fn drop_where(&mut self, gone: impl Fn(&Waiting) -> bool) -> bool {
        let front_gone = self.waiting.first().is_some_and(&gone);
        let (dropped, kept): (Vec<Waiting>, Vec<Waiting>) =
            self.waiting.drain(..).partition(|waiting| gone(waiting));
        self.waiting = kept;
        if front_gone {
            self.picks = Picks::default();
            self.note = None;
        }
        let changed = !dropped.is_empty();
        dropped.into_iter().for_each(Waiting::let_go);
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::channel::oneshot;

    const PLAN_TOOL: &str = "ExitPlanMode";
    const BASH_TOOL: &str = "Bash";

    fn plan() -> Waiting {
        let ask = ToolAsk::held(PLAN_TOOL.to_owned(), <_>::default(), Vec::new());
        Waiting::held(
            ask,
            AskReply::Keys,
            PLAN_TOOL.to_owned(),
            Some(ToolUseId::from("toolu_plan")),
        )
    }

    fn bash(call: Option<&str>) -> (Waiting, oneshot::Receiver<Response>) {
        let (sender, receiver) = oneshot::channel();
        let ask = ToolAsk::held(BASH_TOOL.to_owned(), <_>::default(), Vec::new());
        let waiting = Waiting::held(
            ask,
            AskReply::Hook(sender),
            BASH_TOOL.to_owned(),
            call.map(ToolUseId::from),
        );
        (waiting, receiver)
    }

    fn tools(queue: &AskQueue) -> Vec<Option<&str>> {
        queue
            .waiting
            .iter()
            .map(|waiting| waiting.tool.as_deref())
            .collect()
    }

    #[test]
    fn a_plan_answered_in_the_terminal_settles_once_another_tool_runs() {
        let mut queue = AskQueue::default();
        let (permission, _reply) = bash(None);
        queue.push(plan());
        queue.push(permission);
        assert!(!queue.settle(Settled::PastTool(PLAN_TOOL)));
        assert!(queue.settle(Settled::PastTool("Read")));
        assert_eq!(
            tools(&queue),
            [Some(BASH_TOOL)],
            "a permission still blocking its hook may wait beside a parallel tool"
        );
    }

    #[test]
    fn a_tool_that_ran_settles_its_asks_and_the_plans_behind_it() {
        let mut queue = AskQueue::default();
        let (permission, mut reply) = bash(None);
        queue.push(permission);
        queue.push(plan());
        assert!(queue.settle(Settled::OfTool("Read")));
        assert_eq!(tools(&queue), [Some(BASH_TOOL)]);
        assert!(queue.settle(Settled::OfTool(BASH_TOOL)));
        assert!(queue.is_empty());
        assert_eq!(
            reply.try_recv(),
            Ok(Some(Response::Decided(ToolDecision::Pass))),
            "the settled hook lets Claude Code go on by itself"
        );
    }

    #[test]
    fn a_tool_result_settles_the_ask_of_its_own_call() {
        let mut queue = AskQueue::default();
        let (first, _first_reply) = bash(Some("toolu_1"));
        let (second, _second_reply) = bash(Some("toolu_2"));
        queue.push(first);
        queue.push(second);
        let finished = ToolUseId::from("toolu_2");
        assert!(queue.settle(Settled::Call {
            call: &finished,
            tool: Some(BASH_TOOL),
        }));
        assert_eq!(queue.waiting.len(), 1);
        assert_eq!(
            queue
                .waiting
                .first()
                .and_then(|waiting| waiting.call.as_ref()),
            Some(&ToolUseId::from("toolu_1"))
        );
        let (unnamed, _unnamed_reply) = bash(None);
        queue.push(unnamed);
        let other = ToolUseId::from("toolu_3");
        assert!(queue.settle(Settled::Call {
            call: &other,
            tool: Some(BASH_TOOL),
        }));
        assert_eq!(
            queue.waiting.len(),
            1,
            "an ask without its call id falls back to the tool's name"
        );
    }

    #[test]
    fn leaving_plan_mode_settles_the_plan() {
        let mut queue = AskQueue::default();
        let (permission, _reply) = bash(None);
        queue.push(permission);
        queue.push(plan());
        assert!(queue.settle(Settled::Plans));
        assert_eq!(tools(&queue), [Some(BASH_TOOL)]);
        assert!(queue.settle(Settled::All));
        assert!(queue.is_empty());
    }

    #[test]
    fn a_hook_that_went_away_drops_its_ask() {
        let mut queue = AskQueue::default();
        let (gone, reply) = bash(None);
        let (kept, _kept_reply) = bash(None);
        queue.push(gone);
        queue.push(kept);
        queue.push(plan());
        assert!(!queue.drop_left_callers());
        drop(reply);
        assert!(queue.drop_left_callers());
        assert_eq!(tools(&queue), [Some(BASH_TOOL), Some(PLAN_TOOL)]);
    }
}
