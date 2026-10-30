mod held;
mod picks;
mod queue;
mod view;

use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::{AppContext as _, Context, Entity, Focusable as _, Subscription, Window};
use krad_agents::Response;
use krad_chat::{AskAnswer, AskKind, LiveChat, Question};
use krad_domain::{
    AgentState, AskOutcome, AskVerdict, ChatEntry, PermissionMode, TerminalId, ToolDecision,
};

use super::chat::fold_entry;
use super::surface::{StructuredChat, SurfaceChat};
use super::EnvironmentShell;
pub(super) use picks::Picks;
pub(super) use queue::{AskQueue, AskReply, Waiting};
pub(super) use view::{ask_line, tray, tray_button};

const DENIED_TEXT: &str = "The user declined this";
const KEEP_PLANNING_TEXT: &str = "The user wants to keep planning";
const REASON_PLACEHOLDER: &str = "Tell the agent what to do instead";
const FEEDBACK_PLACEHOLDER: &str = "What should the plan change?";
const OTHER_PLACEHOLDER: &str = "Your own answer";
const NOTE_ROWS: usize = 2;
const ESCAPE: &[u8] = b"\x1b";
const SUBJECT_SEPARATOR: &str = " ";
const QUESTIONS_SEPARATOR: &str = "; ";

pub(super) const OFFERED_MODES: [PermissionMode; 4] = [
    PermissionMode::Manual,
    PermissionMode::AcceptEdits,
    PermissionMode::Plan,
    PermissionMode::Auto,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Choice {
    Allow,
    AllowAlways,
    Deny,
    ApproveAcceptingEdits,
    ApproveAuto,
    ApproveManually,
    KeepPlanning,
}

impl Choice {
    fn of_waiting(waiting: &Waiting) -> Vec<Self> {
        Self::of(
            &waiting.ask.kind,
            waiting.ask.can_allow_always(),
            waiting.by_keys(),
        )
    }

    fn of(kind: &AskKind, can_allow_always: bool, by_keys: bool) -> Vec<Self> {
        match kind {
            AskKind::Permission { .. } => {
                let always = can_allow_always.then_some(Self::AllowAlways);
                [Self::Allow]
                    .into_iter()
                    .chain(always)
                    .chain([Self::Deny])
                    .collect()
            }
            AskKind::Plan { .. } => vec![
                if by_keys {
                    Self::ApproveAuto
                } else {
                    Self::ApproveAcceptingEdits
                },
                Self::ApproveManually,
                Self::KeepPlanning,
            ],
            AskKind::Questions(_) => Vec::new(),
        }
    }

    const fn note(self) -> Option<NotePurpose> {
        match self {
            Self::Deny => Some(NotePurpose::DenyReason),
            Self::KeepPlanning => Some(NotePurpose::PlanFeedback),
            Self::Allow
            | Self::AllowAlways
            | Self::ApproveAcceptingEdits
            | Self::ApproveAuto
            | Self::ApproveManually => None,
        }
    }

    const fn answer(self) -> Option<AskAnswer> {
        Some(match self {
            Self::Allow => AskAnswer::Allow,
            Self::AllowAlways => AskAnswer::AllowAlways,
            Self::ApproveAcceptingEdits => AskAnswer::ApprovePlan {
                mode: PermissionMode::AcceptEdits,
            },
            Self::ApproveAuto => AskAnswer::ApprovePlan {
                mode: PermissionMode::Auto,
            },
            Self::ApproveManually => AskAnswer::ApprovePlan {
                mode: PermissionMode::Manual,
            },
            Self::Deny | Self::KeepPlanning => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum NotePurpose {
    DenyReason,
    PlanFeedback,
    OtherAnswer { question: String },
}

impl NotePurpose {
    const fn placeholder(&self) -> &'static str {
        match self {
            Self::DenyReason => REASON_PLACEHOLDER,
            Self::PlanFeedback => FEEDBACK_PLACEHOLDER,
            Self::OtherAnswer { .. } => OTHER_PLACEHOLDER,
        }
    }
}

struct Note {
    purpose: NotePurpose,
    input: Entity<TextareaState>,
    _events: Subscription,
}

fn front_question(waiting: &[Waiting], text: &str) -> Option<(Vec<Question>, Question)> {
    let AskKind::Questions(questions) = &waiting.first()?.ask.kind else {
        return None;
    };
    let asked = questions.iter().find(|asked| asked.text == text)?.clone();
    Some((questions.clone(), asked))
}

fn subject(kind: &AskKind) -> String {
    match kind {
        AskKind::Permission { tool, summary, .. } => format!("{tool}{SUBJECT_SEPARATOR}{summary}")
            .trim()
            .to_owned(),
        AskKind::Questions(questions) => questions
            .iter()
            .map(|question| question.text.as_str())
            .collect::<Vec<_>>()
            .join(QUESTIONS_SEPARATOR),
        AskKind::Plan { plan } => plan
            .lines()
            .map(|line| line.trim_start_matches('#').trim())
            .find(|line| !line.is_empty())
            .unwrap_or_default()
            .to_owned(),
    }
}

const fn verdict(kind: &AskKind, answer: &AskAnswer) -> AskVerdict {
    match (answer, kind) {
        (AskAnswer::Allow, _) => AskVerdict::Allowed,
        (AskAnswer::AllowAlways, _) => AskVerdict::AllowedAlways,
        (AskAnswer::ApprovePlan { mode }, _) => AskVerdict::PlanApproved(*mode),
        (AskAnswer::Deny { .. }, AskKind::Plan { .. }) => AskVerdict::PlanRejected,
        (AskAnswer::Deny { .. }, _) => AskVerdict::Denied,
        (AskAnswer::Answers(_), _) => AskVerdict::Answered,
    }
}

fn outcome(kind: &AskKind, verdict: AskVerdict) -> AskOutcome {
    AskOutcome {
        subject: subject(kind),
        verdict,
    }
}

impl EnvironmentShell {
    fn structured_of(&self, id: TerminalId) -> Option<&StructuredChat> {
        self.envs.get(&self.active)?.chats.get(&id)?.structured()
    }

    fn structured_of_mut(&mut self, id: TerminalId) -> Option<&mut StructuredChat> {
        let env = self.active.clone();
        self.envs
            .get_mut(&env)?
            .chats
            .get_mut(&id)?
            .structured_mut()
    }

    pub(super) fn ask_queue(&self, owner: TerminalId) -> Option<&AskQueue> {
        self.envs
            .get(&self.active)?
            .chats
            .get(&owner)
            .map(SurfaceChat::asks)
    }

    fn ask_queue_mut(&mut self, owner: TerminalId) -> Option<&mut AskQueue> {
        let env = self.active.clone();
        self.envs
            .get_mut(&env)?
            .chats
            .get_mut(&owner)
            .map(SurfaceChat::asks_mut)
    }

    fn ask_process(&self, owner: TerminalId) -> Option<&LiveChat> {
        self.structured_of(owner)?.live()
    }

    pub(super) fn permission_mode(&self, owner: TerminalId) -> Option<PermissionMode> {
        self.structured_of(owner)?.permission_mode
    }

    pub(super) fn set_permission_mode(
        &mut self,
        owner: TerminalId,
        mode: PermissionMode,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.ask_process(owner) {
            live.set_permission_mode(mode);
        }
        if let Some(structured) = self.structured_of_mut(owner) {
            structured.permission_mode = Some(mode);
        }
        cx.notify();
    }

    fn record_ask(&mut self, owner: TerminalId, outcome: AskOutcome) {
        let entry = ChatEntry::Asked(outcome);
        let env = self.active.clone();
        if let Some(chat) = self
            .envs
            .get_mut(&env)
            .and_then(|live| live.chats.get_mut(&owner))
        {
            fold_entry(&mut chat.messages, &entry);
        }
    }

    fn answer_ask(&mut self, owner: TerminalId, answer: &AskAnswer, cx: &mut Context<Self>) {
        let Some(Waiting { ask, reply, .. }) =
            self.ask_queue_mut(owner).and_then(AskQueue::take_front)
        else {
            return;
        };
        match reply {
            AskReply::Live => {
                if let Some(live) = self.ask_process(owner) {
                    live.answer(&ask, answer);
                }
            }
            AskReply::Hook(sender) => sender
                .send(Response::Decided(ask.decision(answer)))
                .unwrap_or_default(),
            AskReply::Keys => self.type_plan_answer(owner, answer),
        }
        self.record_ask(owner, outcome(&ask.kind, verdict(&ask.kind, answer)));
        let waits = self.ask_queue(owner).is_some_and(|queue| !queue.is_empty());
        let state = if waits {
            AgentState::WaitingForInput
        } else {
            AgentState::Working
        };
        let env = self.active.clone();
        self.set_agent_state(&env, owner, state);
        cx.notify();
    }

    fn choose(
        &mut self,
        owner: TerminalId,
        choice: Choice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(answer) = choice.answer() {
            self.answer_ask(owner, &answer, cx);
        } else if let Some(purpose) = choice.note() {
            self.open_note(owner, purpose, window, cx);
        }
    }

    fn choose_number(
        &mut self,
        owner: TerminalId,
        number: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = number.checked_sub(1) else {
            return;
        };
        let Some(queue) = self.ask_queue_mut(owner) else {
            return;
        };
        let Some(waiting) = queue.waiting.first() else {
            return;
        };
        let AskKind::Questions(questions) = &waiting.ask.kind else {
            let choice = Choice::of_waiting(waiting).get(index).copied();
            if let Some(choice) = choice {
                self.choose(owner, choice, window, cx);
            }
            return;
        };
        let questions = questions.clone();
        let Some(question) = queue.picks.current(&questions).cloned() else {
            return;
        };
        let options: Vec<String> = picks::offered(&question)
            .map(|option| option.label.clone())
            .collect();
        match options.get(index) {
            Some(label) => queue.picks.pick_in(&questions, &question, label),
            None if index == options.len() => {
                let purpose = NotePurpose::OtherAnswer {
                    question: question.text,
                };
                self.open_note(owner, purpose, window, cx);
            }
            None => return,
        }
        cx.notify();
    }

    fn confirm_ask(&mut self, owner: TerminalId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(queue) = self.ask_queue(owner) else {
            return;
        };
        let Some(waiting) = queue.waiting.first() else {
            return;
        };
        match &waiting.ask.kind {
            AskKind::Questions(_) => self.next_question(owner, cx),
            AskKind::Permission { .. } | AskKind::Plan { .. } => {
                let first = Choice::of_waiting(waiting).first().copied();
                if let Some(choice) = first {
                    self.choose(owner, choice, window, cx);
                }
            }
        }
    }

    pub(super) fn deny_and_interrupt(&mut self, owner: TerminalId, cx: &mut Context<Self>) {
        let answer = AskAnswer::Deny {
            message: DENIED_TEXT.to_owned(),
        };
        let reply = self
            .ask_queue(owner)
            .and_then(AskQueue::front)
            .map(|waiting| &waiting.reply);
        match reply {
            None => {}
            Some(AskReply::Live) => {
                self.answer_ask(owner, &answer, cx);
                if let Some(live) = self.ask_process(owner) {
                    live.interrupt();
                }
            }
            Some(AskReply::Hook(_)) => {
                let Some(Waiting { ask, reply, .. }) =
                    self.ask_queue_mut(owner).and_then(AskQueue::take_front)
                else {
                    return;
                };
                if let AskReply::Hook(sender) = reply {
                    let decision = ToolDecision::Deny {
                        message: DENIED_TEXT.to_owned(),
                        interrupt: true,
                    };
                    sender.send(Response::Decided(decision)).unwrap_or_default();
                }
                if matches!(ask.kind, AskKind::Questions(_)) {
                    self.type_into_owner(owner, ESCAPE);
                }
                self.record_ask(owner, outcome(&ask.kind, verdict(&ask.kind, &answer)));
                cx.notify();
            }
            Some(AskReply::Keys) => {
                if let Some(Waiting { ask, .. }) =
                    self.ask_queue_mut(owner).and_then(AskQueue::take_front)
                {
                    self.type_into_owner(owner, ESCAPE);
                    self.record_ask(owner, outcome(&ask.kind, verdict(&ask.kind, &answer)));
                }
                cx.notify();
            }
        }
    }

    fn pick_option(
        &mut self,
        owner: TerminalId,
        question: &str,
        label: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(queue) = self.ask_queue_mut(owner) else {
            return;
        };
        queue.drop_other_answer();
        if let Some((questions, asked)) = front_question(&queue.waiting, question) {
            queue.picks.pick_in(&questions, &asked, label);
        }
        cx.notify();
    }

    fn show_question(&mut self, owner: TerminalId, step: usize, cx: &mut Context<Self>) {
        self.keep_other_answer(owner, cx);
        let Some(queue) = self.ask_queue_mut(owner) else {
            return;
        };
        if let Some(AskKind::Questions(questions)) =
            queue.waiting.first().map(|waiting| &waiting.ask.kind)
        {
            let count = questions.len();
            queue.picks.show(step, count);
        }
        cx.notify();
    }

    fn keep_other_answer(&mut self, owner: TerminalId, cx: &mut Context<Self>) {
        let Some(queue) = self.ask_queue_mut(owner) else {
            return;
        };
        let Some(question) = queue.other_question() else {
            return;
        };
        let typed = queue
            .note
            .as_ref()
            .map(|note| note.input.read(cx).value().trim().to_owned());
        queue.drop_other_answer();
        if let (Some(typed), Some((questions, asked))) = (
            typed.filter(|typed| !typed.is_empty()),
            front_question(&queue.waiting, &question),
        ) {
            queue.picks.pick_in(&questions, &asked, &typed);
        }
    }

    fn next_question(&mut self, owner: TerminalId, cx: &mut Context<Self>) {
        self.keep_other_answer(owner, cx);
        let Some(queue) = self.ask_queue(owner) else {
            return;
        };
        let Some(AskKind::Questions(questions)) =
            queue.waiting.first().map(|waiting| &waiting.ask.kind)
        else {
            return;
        };
        let step = queue.picks.step(questions.len());
        if step + 1 < questions.len() {
            self.show_question(owner, step + 1, cx);
        } else if let Some(unanswered) = queue.picks.unanswered(questions) {
            self.show_question(owner, unanswered, cx);
        } else {
            self.submit_picks(owner, cx);
        }
    }

    fn submit_picks(&mut self, owner: TerminalId, cx: &mut Context<Self>) {
        let Some(queue) = self.ask_queue(owner) else {
            return;
        };
        let answer = AskAnswer::Answers(queue.picks.answers());
        self.answer_ask(owner, &answer, cx);
    }

    fn open_note(
        &mut self,
        owner: TerminalId,
        purpose: NotePurpose,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let placeholder = purpose.placeholder();
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(NOTE_ROWS)
                .submit_on_enter(true)
                .placeholder(placeholder)
        });
        let events = cx.subscribe_in(
            &input,
            window,
            move |this: &mut Self, _, event: &InputEvent, _, cx| {
                if let InputEvent::PressEnter { shift: false, .. } = event {
                    this.submit_note(owner, cx);
                }
            },
        );
        input.update(cx, |field, cx| field.focus(window, cx));
        if let Some(queue) = self.ask_queue_mut(owner) {
            queue.note = Some(Note {
                purpose,
                input,
                _events: events,
            });
        }
        cx.notify();
    }

    fn submit_note(&mut self, owner: TerminalId, cx: &mut Context<Self>) {
        let Some(queue) = self.ask_queue_mut(owner) else {
            return;
        };
        let Some(note) = queue.note.take() else {
            return;
        };
        let text = note.input.read(cx).value().trim().to_owned();
        let message = |fallback: &str| {
            if text.is_empty() {
                fallback.to_owned()
            } else {
                text.clone()
            }
        };
        match note.purpose {
            NotePurpose::DenyReason => {
                let answer = AskAnswer::Deny {
                    message: message(DENIED_TEXT),
                };
                self.answer_ask(owner, &answer, cx);
            }
            NotePurpose::PlanFeedback => {
                let answer = AskAnswer::Deny {
                    message: message(KEEP_PLANNING_TEXT),
                };
                self.answer_ask(owner, &answer, cx);
            }
            NotePurpose::OtherAnswer { question } => {
                let asked = front_question(&queue.waiting, &question);
                if let (Some((questions, asked)), false) = (asked, text.is_empty()) {
                    queue.picks.pick_in(&questions, &asked, &text);
                }
                cx.notify();
            }
        }
    }

    pub(super) fn sync_ask_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let owners: Vec<TerminalId> = self
            .envs
            .get(&self.active)
            .map_or_default(|live| live.chats.keys().copied().collect());
        for owner in owners {
            self.sync_owner_focus(owner, window, cx);
        }
    }

    fn sync_owner_focus(&mut self, owner: TerminalId, window: &mut Window, cx: &mut Context<Self>) {
        let composer = self.ask_composer(owner);
        let composer_focused = composer
            .as_ref()
            .is_some_and(|input| input.focus_handle(cx).is_focused(window));
        let Some(queue) = self.ask_queue_mut(owner) else {
            return;
        };
        if queue.is_empty() {
            let ask_focused = queue
                .focus
                .as_ref()
                .is_some_and(|focus| focus.contains_focused(window, cx));
            if let (true, Some(input)) = (ask_focused, composer) {
                input.update(cx, |field, cx| field.focus(window, cx));
            }
            return;
        }
        let focus = queue.focus.get_or_insert_with(|| cx.focus_handle()).clone();
        if std::mem::take(&mut queue.claims_focus) && composer_focused {
            window.focus(&focus, cx);
        }
    }

    fn ask_composer(&self, owner: TerminalId) -> Option<Entity<TextareaState>> {
        self.envs
            .get(&self.active)?
            .chats
            .get(&owner)
            .map(|chat| chat.input.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn permission(tool: &str, summary: &str) -> AskKind {
        AskKind::Permission {
            tool: tool.to_owned(),
            kind: krad_domain::ToolKind::of_claude_code(tool),
            summary: summary.to_owned(),
            title: None,
            edit: None,
        }
    }

    #[test]
    fn a_permission_offers_allow_always_only_with_a_suggestion() {
        let bash = permission("Bash", "ls");
        assert_eq!(
            Choice::of(&bash, false, false),
            [Choice::Allow, Choice::Deny]
        );
        assert_eq!(
            Choice::of(&bash, true, false),
            [Choice::Allow, Choice::AllowAlways, Choice::Deny]
        );
        assert_eq!(
            outcome(&bash, verdict(&bash, &AskAnswer::AllowAlways)),
            AskOutcome {
                subject: "Bash ls".to_owned(),
                verdict: AskVerdict::AllowedAlways,
            }
        );
    }

    #[test]
    fn a_plan_reads_as_its_first_heading_and_a_denial_sends_it_back() {
        let plan = AskKind::Plan {
            plan: "\n# Add the notes file\n- Write it".to_owned(),
        };
        assert_eq!(subject(&plan), "Add the notes file");
        let sent_back = AskAnswer::Deny {
            message: KEEP_PLANNING_TEXT.to_owned(),
        };
        assert_eq!(verdict(&plan, &sent_back), AskVerdict::PlanRejected);
        assert_eq!(
            Choice::of(&plan, false, false)
                .first()
                .copied()
                .and_then(Choice::answer),
            Some(AskAnswer::ApprovePlan {
                mode: PermissionMode::AcceptEdits,
            }),
            "Enter approves and accepts edits, as in Claude Code"
        );
        assert_eq!(Choice::KeepPlanning.answer(), None, "it takes a note first");
    }
}
