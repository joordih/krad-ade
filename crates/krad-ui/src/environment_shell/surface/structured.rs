mod acp;
mod checkpoint;
mod flush;
mod forks;
mod mcp;
mod open;
mod orchestrator;
mod rewind;
mod runtime_chat;

use krad_chat::{new_session_id, Catalog, CommandOffer, LiveSession, SlashCommand};
use krad_domain::{
    AgentPreset, Effort, EffortSnap, EnvironmentName, McpServerName, ModelId, PermissionMode,
    ResumeId, TerminalId,
};
use krad_store::acp_transcript_file;

use super::super::ask::AskQueue;
use super::super::EnvironmentShell;
use super::acp::AcpOffers;
use super::outbox::Outbox;
use super::{Backend, ChatView, SurfaceCommand};

pub(super) use acp::read_acp_env;
pub(in crate::environment_shell) use acp::read_acp_secrets;
pub(in crate::environment_shell) use open::restored_chats;
pub(in crate::environment_shell) use runtime_chat::ChatDriver;
use runtime_chat::RuntimeShown;

const FULL_PERCENT: u64 = 100;
const THINKING_DEFAULT: bool = true;
pub(in crate::environment_shell) struct StructuredChat {
    pub(in crate::environment_shell) harness: AgentPreset,
    pub(super) session: LiveSession,
    pub(in crate::environment_shell) asks: AskQueue,
    pub(in crate::environment_shell) permission_mode: Option<PermissionMode>,
    pub(in crate::environment_shell) catalog: Option<Catalog>,
    pub(super) model: Option<String>,
    pub(in crate::environment_shell) chosen_model: Option<ModelId>,
    pub(in crate::environment_shell) effort: Option<Effort>,
    pub(super) effort_notice: Option<EffortSnap>,
    pub(super) effort_was: Option<Effort>,
    pub(super) thinking: bool,
    commands: Vec<SlashCommand>,
    context_tokens: Option<u64>,
    pub(super) context_window: Option<u64>,
    pub(super) working: bool,
    mcp_servers: Option<Vec<McpServerName>>,
    notices: Vec<String>,
    rewinding: bool,
    pub(in crate::environment_shell) acp: AcpOffers,
    driver: ChatDriver,
    shown: RuntimeShown,
}

impl StructuredChat {
    fn new(session: LiveSession, harness: AgentPreset) -> Self {
        Self {
            harness,
            session,
            asks: AskQueue::default(),
            permission_mode: None,
            catalog: None,
            model: None,
            chosen_model: None,
            effort: None,
            effort_notice: None,
            effort_was: None,
            thinking: THINKING_DEFAULT,
            commands: Vec::new(),
            context_tokens: None,
            context_window: None,
            working: false,
            mcp_servers: None,
            notices: Vec::new(),
            rewinding: false,
            acp: AcpOffers::default(),
            driver: ChatDriver::InShell,
            shown: RuntimeShown::default(),
        }
    }

    pub(super) fn is_fresh(&self) -> bool {
        self.session.history().is_none() && !self.working
    }

    pub(super) fn switch_harness(&mut self, harness: AgentPreset) {
        self.harness = harness;
        self.catalog = None;
        self.model = None;
        self.effort = None;
        self.effort_notice = None;
        self.context_tokens = None;
        self.context_window = None;
        self.acp = AcpOffers::default();
        self.session = LiveSession::Start(new_session_id());
    }

    pub(in crate::environment_shell) fn adopt_catalog(
        &mut self,
        commands: Vec<CommandOffer>,
        account: Option<String>,
    ) {
        self.catalog = Some(Catalog {
            models: Vec::new(),
            commands,
            subscription: account,
        });
    }

    pub(super) fn reports_context(&self) -> bool {
        self.harness.acp_agent().is_none()
            || self.context_tokens.is_some()
            || self.context_window.is_some()
    }

    pub(super) fn context_used(&self) -> Option<u16> {
        let (tokens, window) = (self.context_tokens?, self.context_window?);
        let percent = tokens.saturating_mul(FULL_PERCENT) / window.max(1);
        u16::try_from(percent).ok()
    }

    fn has_work(&self, outbox: &Outbox) -> bool {
        (self.in_runtime() || self.is_free()) && (!self.commands.is_empty() || outbox.has_queued())
    }

    pub(super) fn is_free(&self) -> bool {
        !self.working && self.asks.is_empty()
    }

    pub(super) fn takes_bus_notice(&self, outbox: &Outbox) -> bool {
        self.is_free() && !outbox.has_queued() && self.shown.queued.is_empty()
    }

    fn session_reported(&mut self, resume: ResumeId) {
        self.session = LiveSession::Resume(resume);
    }

    fn ends_turn(&mut self) {
        self.working = false;
    }

    pub(super) fn command(&mut self, command: SurfaceCommand) {
        match command {
            SurfaceCommand::Stop => {}
            SurfaceCommand::Model(_, model) => self.chosen_model = Some(model),
            SurfaceCommand::Effort(effort) => {
                self.effort_was = self.effort;
                self.effort = Some(effort);
                self.effort_notice = None;
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
) -> Option<&'a mut ChatView> {
    shell
        .envs
        .get_mut(env)?
        .chats
        .get_mut(&id)
        .filter(|chat| matches!(chat.chat.backend, Backend::Structured(_)))
}

fn structured_mut<'a>(
    shell: &'a mut EnvironmentShell,
    env: &EnvironmentName,
    id: TerminalId,
) -> Option<&'a mut StructuredChat> {
    match &mut structured_chat(shell, env, id)?.chat.backend {
        Backend::Structured(structured) => Some(structured),
        Backend::Transcript(_) => None,
    }
}

impl EnvironmentShell {
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
}

#[cfg(test)]
mod tests;
