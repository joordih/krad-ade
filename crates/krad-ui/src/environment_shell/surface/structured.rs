mod flush;
mod forks;
mod mcp;
mod open;
mod orchestrator;
mod rewind;

use gpui_kit::Context;
use krad_chat::{
    is_missing_conversation, new_session_id, Catalog, ChatEvent, LiveChat, LiveEvent, LiveSession,
    PlanUsage, SlashCommand,
};
use krad_domain::{
    AgentPreset, AgentState, ChatEntry, Effort, EnvironmentName, ModelId, PermissionMode, Prompt,
    ResumeId, TerminalId,
};
use std::time::Instant;

use super::super::ask::{AskQueue, Waiting};
use super::super::chat::{fold_entry, LiveProcess};
use super::super::EnvironmentShell;
use super::outbox::Outbox;
use super::{view, Backend, SurfaceChat, SurfaceCommand};

use mcp::McpPrep;
pub(in crate::environment_shell) use open::restored_chats;
use open::Earlier;
use rewind::Rewinding;

const FULL_PERCENT: u64 = 100;
const THINKING_DEFAULT: bool = true;
const NOTICE_SEPARATOR: &str = "\n";

fn add_notice(problem: &mut Option<String>, notice: &str) {
    match problem {
        Some(shown) => {
            shown.push_str(NOTICE_SEPARATOR);
            shown.push_str(notice);
        }
        None => *problem = Some(notice.to_owned()),
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Batch {
    commands: Vec<SlashCommand>,
    message: Option<Prompt>,
}

impl Batch {
    fn results(&self) -> usize {
        self.commands.len() + usize::from(self.message.is_some())
    }
}

pub(in crate::environment_shell) struct StructuredChat {
    pub(in crate::environment_shell) harness: AgentPreset,
    pub(super) live: Option<LiveProcess>,
    pub(super) session: LiveSession,
    fell_back: bool,
    last_batch: Option<Batch>,
    outstanding: usize,
    pub(in crate::environment_shell) asks: AskQueue,
    pub(in crate::environment_shell) permission_mode: Option<PermissionMode>,
    pub(in crate::environment_shell) catalog: Option<Catalog>,
    pub(super) model: Option<String>,
    pub(super) chosen_model: Option<ModelId>,
    pub(super) effort: Option<Effort>,
    pub(super) thinking: bool,
    commands: Vec<SlashCommand>,
    context_tokens: Option<u64>,
    pub(super) context_window: Option<u64>,
    pub(super) working: bool,
    pub(super) usage: Option<PlanUsage>,
    pub(super) earlier: Option<Earlier>,
    mcp: McpPrep,
    notices: Vec<String>,
    rewinding: Option<Rewinding>,
}

impl StructuredChat {
    fn new(session: LiveSession, harness: AgentPreset) -> Self {
        Self {
            harness,
            live: None,
            session,
            fell_back: false,
            last_batch: None,
            outstanding: 0,
            asks: AskQueue::default(),
            permission_mode: None,
            catalog: None,
            model: None,
            chosen_model: None,
            effort: None,
            thinking: THINKING_DEFAULT,
            commands: Vec::new(),
            context_tokens: None,
            context_window: None,
            working: false,
            usage: None,
            earlier: None,
            mcp: McpPrep::default(),
            notices: Vec::new(),
            rewinding: None,
        }
    }

    pub(super) fn is_fresh(&self) -> bool {
        self.session.history().is_none()
            && self.outstanding == 0
            && !self.live.as_ref().is_some_and(|live| live.confirmed)
    }

    pub(super) fn switch_harness(&mut self, harness: AgentPreset) {
        self.harness = harness;
        self.live = None;
        self.mcp = McpPrep::default();
        self.catalog = None;
        self.model = None;
        self.effort = None;
        self.context_tokens = None;
        self.context_window = None;
        self.session = LiveSession::Start(new_session_id());
    }

    pub(super) fn context_used(&self) -> Option<u16> {
        let (tokens, window) = (self.context_tokens?, self.context_window?);
        let percent = tokens.saturating_mul(FULL_PERCENT) / window.max(1);
        u16::try_from(percent).ok()
    }

    pub(in crate::environment_shell) fn live(&self) -> Option<&LiveChat> {
        self.live.as_ref().and_then(LiveProcess::alive)
    }

    fn has_work(&self, outbox: &Outbox) -> bool {
        self.is_free() && (!self.commands.is_empty() || outbox.has_queued())
    }

    pub(super) fn is_free(&self) -> bool {
        !self.working && self.asks.is_empty()
    }

    pub(super) fn takes_bus_notice(&self, outbox: &Outbox) -> bool {
        self.is_free() && !outbox.has_queued()
    }

    fn take_batch(&mut self, outbox: &mut Outbox, now: Instant) -> Batch {
        Batch {
            commands: std::mem::take(&mut self.commands),
            message: outbox.next_to_send(true, now),
        }
    }

    fn handed_over(&mut self, batch: Batch) {
        self.outstanding += batch.results();
        self.working |= batch.message.is_some();
        self.last_batch = Some(batch);
    }

    fn session_reported(&mut self, resume: ResumeId) {
        if let Some(live) = self.live.as_mut() {
            live.confirmed = true;
        }
        self.session = LiveSession::Resume(resume);
    }

    fn closes_turn(&mut self, ended: bool) -> bool {
        self.outstanding = self.outstanding.saturating_sub(1);
        if ended {
            self.outstanding = 0;
        }
        if self.outstanding > 0 {
            return false;
        }
        self.working = false;
        true
    }

    fn falls_back(&mut self, error: &str) -> Option<Batch> {
        let reported = self
            .live
            .as_ref()
            .is_some_and(|live| live.confirmed || live.chat.is_alive());
        if self.fell_back
            || reported
            || self.session.history().is_none()
            || !is_missing_conversation(error)
        {
            return None;
        }
        self.fell_back = true;
        self.session = LiveSession::Start(new_session_id());
        self.live = None;
        self.outstanding = 0;
        Some(self.last_batch.take().unwrap_or_default())
    }

    pub(super) fn command(&mut self, command: SurfaceCommand) {
        match command {
            SurfaceCommand::Stop => {
                if let Some(live) = self.live() {
                    live.interrupt();
                }
            }
            SurfaceCommand::Model(_, model) => {
                if let Some(live) = self.live() {
                    live.set_model(&model);
                }
                self.chosen_model = Some(model);
            }
            SurfaceCommand::Effort(effort) => {
                self.effort = Some(effort);
                if self.live.is_some() {
                    self.commands.push(SlashCommand::Effort(effort));
                }
            }
            SurfaceCommand::ToggleThinking => {
                self.thinking = !self.thinking;
                self.commands.push(SlashCommand::Thinking(self.thinking));
            }
        }
    }
}

fn structured_chat<'a>(
    shell: &'a mut EnvironmentShell,
    env: &EnvironmentName,
    id: TerminalId,
) -> Option<&'a mut SurfaceChat> {
    shell
        .envs
        .get_mut(env)?
        .chats
        .get_mut(&id)
        .filter(|chat| matches!(chat.backend, Backend::Structured(_)))
}

fn structured_mut<'a>(
    shell: &'a mut EnvironmentShell,
    env: &EnvironmentName,
    id: TerminalId,
) -> Option<&'a mut StructuredChat> {
    match &mut structured_chat(shell, env, id)?.backend {
        Backend::Structured(structured) => Some(structured),
        Backend::Transcript(_) => None,
    }
}

impl EnvironmentShell {
    fn apply_structured_event(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        event: LiveEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(SurfaceChat {
            backend: Backend::Structured(structured),
            messages,
            thread,
            problem,
            ..
        }) = structured_chat(self, env, id)
        else {
            return;
        };
        let followed = view::following(thread);
        let mut state = None;
        match event {
            LiveEvent::Ready(catalog) => {
                let harness = structured.harness;
                self.keep_harness_models(harness, &catalog.models);
                if let Some(structured) = structured_mut(self, env, id) {
                    structured.catalog = Some(catalog);
                }
                return self.flush_structured(env, id, cx);
            }
            LiveEvent::Usage(usage) => structured.usage = Some(usage),
            LiveEvent::Rewound(outcome) => {
                self.rewound(env, id, outcome);
                return self.flush_structured(env, id, cx);
            }
            LiveEvent::Notice(notice) => add_notice(problem, &notice),
            LiveEvent::Ask(ask) => {
                structured.asks.push(Waiting::live(*ask));
                state = Some(AgentState::WaitingForInput);
            }
            LiveEvent::Chat(ChatEvent::Session(resume)) => {
                structured.session_reported(resume.clone());
                self.note_chat_resume(env, id, resume);
                return;
            }
            LiveEvent::Chat(ChatEvent::Model(model)) => structured.model = Some(model),
            LiveEvent::Chat(ChatEvent::PermissionMode(mode)) => {
                structured.permission_mode = Some(mode);
            }
            LiveEvent::Chat(ChatEvent::ContextTokens(tokens)) => {
                structured.context_tokens = Some(tokens);
            }
            LiveEvent::Chat(ChatEvent::ContextWindow(window)) => {
                structured.context_window = Some(window);
            }
            LiveEvent::Chat(event @ (ChatEvent::Done | ChatEvent::Error(_))) => {
                if let ChatEvent::Error(error) = &event {
                    let again = structured
                        .drops_turn_again(error)
                        .or_else(|| structured.falls_back(error));
                    if let Some(batch) = again {
                        self.hand_over_again(env, id, batch, event, cx);
                        return;
                    }
                }
                let ended = structured.live().is_none();
                if structured.closes_turn(ended) {
                    for outcome in structured.asks.cancel_all() {
                        fold_entry(messages, &ChatEntry::Asked(outcome));
                    }
                    fold_entry(messages, &ChatEntry::Event(event));
                    state = Some(AgentState::Idle);
                } else if let ChatEvent::Error(error) = event {
                    *problem = Some(error);
                }
            }
            LiveEvent::Chat(event) => fold_entry(messages, &ChatEntry::Event(event)),
        }
        if followed {
            thread.scroll_to_end();
        }
        if let Some(state) = state {
            self.set_agent_state(env, id, state);
        }
        self.flush_structured(env, id, cx);
    }
}

#[cfg(test)]
mod tests;
