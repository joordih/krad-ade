use gpui_kit::FocusHandle;
use krad_chat::{AskKind, ToolAsk};
use krad_domain::{AskOutcome, AskVerdict, ToolDecision, ToolUseId};
use krad_protocol::{AskId, ChatAsk, ChatAskId};
use krad_runtime::HeldAsk;

use super::{outcome, Note, NotePurpose, Picks};

pub(in crate::environment_shell) enum AskReply {
    Live,
    Hook(HeldAsk),
    Keys,
    Runtime { ask: ChatAskId, allows_always: bool },
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

    pub(in crate::environment_shell) fn of_runtime(asked: ChatAsk) -> Self {
        let mut ask = ToolAsk::held(String::new(), <_>::default(), Vec::new());
        ask.kind = asked.kind;
        Self {
            ask,
            reply: AskReply::Runtime {
                ask: asked.ask,
                allows_always: asked.can_allow_always,
            },
            tool: None,
            call: None,
        }
    }

    pub(super) fn can_allow_always(&self) -> bool {
        match &self.reply {
            AskReply::Runtime { allows_always, .. } => *allows_always,
            AskReply::Live | AskReply::Hook(_) | AskReply::Keys => self.ask.can_allow_always(),
        }
    }

    fn is_chat_ask(&self, ask: ChatAskId) -> bool {
        matches!(&self.reply, AskReply::Runtime { ask: held, .. } if *held == ask)
    }

    pub(super) const fn by_keys(&self) -> bool {
        matches!(self.reply, AskReply::Keys)
    }

    fn is_ask(&self, ask: AskId) -> bool {
        matches!(&self.reply, AskReply::Hook(held) if held.ask == ask)
    }

    fn let_go(self) {
        if let AskReply::Hook(held) = self.reply {
            held.decide(ToolDecision::Pass);
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

    pub(in crate::environment_shell) fn holds_chat_ask(&self, ask: ChatAskId) -> bool {
        self.waiting.iter().any(|waiting| waiting.is_chat_ask(ask))
    }

    pub(in crate::environment_shell) fn forget_chat_ask(&mut self, ask: ChatAskId) -> bool {
        self.drop_where(|waiting| waiting.is_chat_ask(ask))
    }

    pub(in crate::environment_shell) fn forget_ask(&mut self, ask: AskId) -> bool {
        let front_gone = self
            .waiting
            .first()
            .is_some_and(|waiting| waiting.is_ask(ask));
        let before = self.waiting.len();
        self.waiting.retain(|waiting| !waiting.is_ask(ask));
        if front_gone {
            self.picks = Picks::default();
            self.note = None;
        }
        self.waiting.len() != before
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
    use krad_runtime::{Runtime, RuntimeHandle, RuntimeSetup};

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

    fn runtime() -> RuntimeHandle {
        let started = Runtime::start(RuntimeSetup::default());
        let Ok(runtime) = started else {
            unreachable!("the test runtime starts its thread");
        };
        runtime
    }

    fn bash(runtime: &RuntimeHandle, ask: u64, call: Option<&str>) -> Waiting {
        let held = runtime.held_ask(AskId(ask));
        let asked = ToolAsk::held(BASH_TOOL.to_owned(), <_>::default(), Vec::new());
        Waiting::held(
            asked,
            AskReply::Hook(held),
            BASH_TOOL.to_owned(),
            call.map(ToolUseId::from),
        )
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
        let runtime = runtime();
        let mut queue = AskQueue::default();
        queue.push(plan());
        queue.push(bash(&runtime, 1, None));
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
        let runtime = runtime();
        let mut queue = AskQueue::default();
        queue.push(bash(&runtime, 1, None));
        queue.push(plan());
        assert!(queue.settle(Settled::OfTool("Read")));
        assert_eq!(tools(&queue), [Some(BASH_TOOL)]);
        assert!(queue.settle(Settled::OfTool(BASH_TOOL)));
        assert!(queue.is_empty());
    }

    #[test]
    fn a_tool_result_settles_the_ask_of_its_own_call() {
        let runtime = runtime();
        let mut queue = AskQueue::default();
        queue.push(bash(&runtime, 1, Some("toolu_1")));
        queue.push(bash(&runtime, 2, Some("toolu_2")));
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
        queue.push(bash(&runtime, 3, None));
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
        let runtime = runtime();
        let mut queue = AskQueue::default();
        queue.push(bash(&runtime, 1, None));
        queue.push(plan());
        assert!(queue.settle(Settled::Plans));
        assert_eq!(tools(&queue), [Some(BASH_TOOL)]);
        assert!(queue.settle(Settled::All));
        assert!(queue.is_empty());
    }

    #[test]
    fn an_ask_the_runtime_settled_leaves_the_queue() {
        let runtime = runtime();
        let mut queue = AskQueue::default();
        queue.push(bash(&runtime, 1, None));
        queue.push(bash(&runtime, 2, None));
        queue.push(plan());
        assert!(!queue.forget_ask(AskId(9)));
        assert!(queue.forget_ask(AskId(1)));
        assert_eq!(tools(&queue), [Some(BASH_TOOL), Some(PLAN_TOOL)]);
    }
}
