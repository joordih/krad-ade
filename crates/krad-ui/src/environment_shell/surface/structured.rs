mod acp;
mod flush;
mod forks;
mod mcp;
mod open;
mod orchestrator;
mod rewind;

use gpui_kit::Context;
use krad_chat::{
    new_session_id, AcpEvent, Catalog, ChatEvent, LiveChat, LiveEvent, LiveSession, PlanUsage,
    ResumeRefused, SlashCommand,
};
use krad_domain::{
    AgentPreset, AgentState, ChatEntry, Effort, EnvironmentName, Mention, ModelId, PermissionMode,
    Prompt, ResumeId, TerminalId,
};
use krad_store::{acp_transcript_file, AcpTranscript};
use std::time::Instant;

use super::super::ask::{AskQueue, Waiting};
use super::super::chat::{fold_entry, fold_prompt, ChatMessage, LiveProcess};
use super::super::EnvironmentShell;
use super::acp::AcpOffers;
use super::outbox::Outbox;
use super::{view, Backend, SurfaceChat, SurfaceCommand};

pub(super) use acp::read_acp_env;
use acp::AcpSecretsPrep;
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
    acp_secrets: AcpSecretsPrep,
    notices: Vec<String>,
    rewinding: Option<Rewinding>,
    pub(in crate::environment_shell) acp: AcpOffers,
    transcript: Option<AcpTranscript>,
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
            acp_secrets: AcpSecretsPrep::default(),
            notices: Vec::new(),
            rewinding: None,
            acp: AcpOffers::default(),
            transcript: None,
        }
    }

    pub(super) fn is_fresh(&self) -> bool {
        self.session.history().is_none()
            && self.outstanding == 0
            && !self.live.as_ref().is_some_and(|live| live.confirmed)
    }

    pub(in crate::environment_shell) fn fold_kept(
        &self,
        messages: &mut Vec<ChatMessage>,
        entry: &ChatEntry,
    ) {
        if let Some(transcript) = &self.transcript {
            transcript.keep(entry);
        }
        fold_entry(messages, entry);
    }

    fn fold_sent(&self, messages: &mut Vec<ChatMessage>, prompt: &Prompt) {
        if let Some(transcript) = &self.transcript {
            transcript.keep(&ChatEntry::User {
                text: prompt.text.clone(),
                preset: self.harness,
                model: self.chosen_model.clone(),
                images: prompt
                    .mentions
                    .iter()
                    .filter_map(|mention| match mention {
                        Mention::Image(image) => Some(image.clone()),
                        _ => None,
                    })
                    .collect(),
            });
        }
        fold_prompt(messages, prompt, self.harness);
    }

    pub(super) fn switch_harness(
        &mut self,
        harness: AgentPreset,
        transcript: Option<AcpTranscript>,
    ) {
        self.transcript = transcript;
        self.harness = harness;
        self.live = None;
        self.mcp = McpPrep::default();
        self.acp_secrets = AcpSecretsPrep::default();
        self.catalog = None;
        self.model = None;
        self.effort = None;
        self.context_tokens = None;
        self.context_window = None;
        self.acp = AcpOffers::default();
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

    fn retries(&mut self, refused: &ResumeRefused) -> Option<Batch> {
        match refused {
            ResumeRefused::MissingConversation(_) => {
                let batch = self.falls_back()?;
                if matches!(self.harness, AgentPreset::Acp(_)) {
                    self.notices.push(refused.to_string());
                }
                Some(batch)
            }
            ResumeRefused::TurnNotDropped(_) => self.drops_turn_again(),
        }
    }

    fn falls_back(&mut self) -> Option<Batch> {
        let reported = self
            .live
            .as_ref()
            .is_some_and(|live| live.confirmed || live.chat.is_alive());
        if self.fell_back || reported || self.session.history().is_none() {
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
    pub(super) fn acp_transcript(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
        harness: AgentPreset,
    ) -> Option<AcpTranscript> {
        if !matches!(harness, AgentPreset::Acp(_)) {
            return None;
        }
        self.acp_transcripts
            .as_ref()?
            .transcript(acp_transcript_file(env, id)?)
    }

    pub(in crate::environment_shell) fn forget_acp_transcript(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
    ) {
        let (Some(transcripts), Some(file)) = (&self.acp_transcripts, acp_transcript_file(env, id))
        else {
            return;
        };
        transcripts.forget(file);
    }

    fn apply_structured_event(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        event: LiveEvent,
        cx: &mut Context<Self>,
    ) {
        if self.follow_worker_launch(env, id, &event, cx) {
            return;
        }
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
        let (event, refused) = match event {
            LiveEvent::Refused(refused) => (
                LiveEvent::Chat(ChatEvent::Error(refused.to_string())),
                Some(refused),
            ),
            event => (event, None),
        };
        match event {
            LiveEvent::Ready(catalog) => {
                let harness = structured.harness;
                self.keep_harness_models(env, harness, &catalog.models);
                if let Some(structured) = structured_mut(self, env, id) {
                    structured.catalog = Some(catalog);
                    structured.acp.opened();
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
            LiveEvent::Acp(event) => {
                if matches!(event, AcpEvent::SignInNeeded(_)) {
                    state = Some(AgentState::WaitingForInput);
                }
                structured.acp.reported(event);
            }
            LiveEvent::Refused(_) => {}
            LiveEvent::Chat(event @ (ChatEvent::Done | ChatEvent::Error(_))) => {
                if let Some(batch) = refused.and_then(|refused| structured.retries(&refused)) {
                    self.hand_over_again(env, id, batch, event, cx);
                    return;
                }
                let ended = structured.live().is_none();
                if structured.closes_turn(ended) {
                    for outcome in structured.asks.cancel_all() {
                        structured.fold_kept(messages, &ChatEntry::Asked(outcome));
                    }
                    structured.fold_kept(messages, &ChatEntry::Event(event));
                    state = Some(AgentState::Idle);
                } else if let ChatEvent::Error(error) = event {
                    *problem = Some(error);
                }
            }
            LiveEvent::Chat(event) => structured.fold_kept(messages, &ChatEntry::Event(event)),
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
