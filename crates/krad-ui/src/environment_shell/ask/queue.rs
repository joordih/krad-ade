use super::{HookAsk, Note, NotePurpose, Picks};
use gpui_kit::FocusHandle;
use krad_domain::{AskKind, ToolDecision, ToolUseId};
use krad_protocol::{AskId, ChatAsk, ChatAskId};

pub(in crate::environment_shell) enum AskReply {
    Hook(HookAsk),
    Keys,
    Runtime(ChatAskId),
}

pub(in crate::environment_shell) struct Waiting {
    pub(super) kind: AskKind,
    allows_always: bool,
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
            Self::Plans => waiting.by_keys() && matches!(waiting.kind, AskKind::Plan { .. }),
        }
    }
}

impl Waiting {
    pub(in crate::environment_shell) const fn held(
        kind: AskKind,
        allows_always: bool,
        reply: AskReply,
        tool: String,
        call: Option<ToolUseId>,
    ) -> Self {
        Self {
            kind,
            allows_always,
            reply,
            tool: Some(tool),
            call,
        }
    }

    pub(in crate::environment_shell) fn of_runtime(asked: ChatAsk) -> Self {
        Self {
            kind: asked.kind,
            allows_always: asked.can_allow_always,
            reply: AskReply::Runtime(asked.ask),
            tool: None,
            call: None,
        }
    }

    pub(super) const fn can_allow_always(&self) -> bool {
        self.allows_always
    }

    fn is_chat_ask(&self, ask: ChatAskId) -> bool {
        matches!(&self.reply, AskReply::Runtime(held) if *held == ask)
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
            .is_some_and(|waiting| matches!(waiting.kind, AskKind::Questions(_)))
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
    use crate::fake_client::RecordedClient;
    use krad_protocol::{BusCommand, Command};
    use std::sync::Arc;

    const PLAN_TOOL: &str = "ExitPlanMode";
    const BASH_TOOL: &str = "Bash";

    fn plan() -> Waiting {
        let kind = AskKind::Plan {
            plan: String::new(),
        };
        Waiting::held(
            kind,
            false,
            AskReply::Keys,
            PLAN_TOOL.to_owned(),
            Some(ToolUseId::from("toolu_plan")),
        )
    }

    fn runtime() -> Arc<RecordedClient> {
        RecordedClient::new(Vec::new())
    }

    fn bash(runtime: &Arc<RecordedClient>, ask: u64, call: Option<&str>) -> Waiting {
        let held = HookAsk::new(AskId(ask), Arc::clone(runtime) as _);
        let kind = AskKind::Permission {
            tool: BASH_TOOL.to_owned(),
            kind: krad_domain::ToolKind::Shell,
            summary: String::new(),
            title: None,
            edit: None,
        };
        Waiting::held(
            kind,
            true,
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
        assert_eq!(runtime.sent(), []);
        assert!(queue.settle(Settled::All));
        assert!(queue.is_empty());
        assert_eq!(
            runtime.sent(),
            [Command::Bus(BusCommand::AnswerAsk {
                ask: AskId(1),
                decision: ToolDecision::Pass,
            })],
            "a hook ask the queue lets go is passed back to its agent"
        );
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
