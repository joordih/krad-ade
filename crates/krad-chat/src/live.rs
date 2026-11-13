mod claude;

use krad_domain::{
    AcpAuthMethodId, AcpConfigOptionId, AcpConfigValue, AcpModeId, Effort, GatewayAccess, ModelId,
    PermissionMode, Prompt, UsageBudget,
};
use std::fmt;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use crate::acp::{spawn_acp, AcpConfig};
use crate::codex::spawn_codex;
use crate::control::{AskAnswer, LiveEvent, ToolAsk};
use crate::engine::{spawn_engine, EngineConfig};
use crate::error::ChatError;
use crate::message::SlashCommand;
use crate::opencode::{spawn_opencode, OpencodeServer};
use crate::process::Runner;
use crate::rewind::RewindRequest;
use claude::spawn_claude;
pub(crate) use krad_domain::new_uuid;
pub use krad_domain::{new_session_id, LiveSession};

const STDERR_TAIL_CHARS: usize = 600;
#[derive(Clone, Default, PartialEq, Eq)]
pub struct BusVars(pub Vec<(String, String)>);

impl fmt::Debug for BusVars {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_list()
            .entries(self.0.iter().map(|(name, _)| name))
            .finish()
    }
}

#[derive(Clone, Debug)]
pub struct LiveConfig {
    pub runner: Runner,
    pub model: Option<ModelId>,
    pub cwd: PathBuf,
    pub system_prompt: Option<String>,
    pub effort: Option<Effort>,
    pub permission_mode: Option<PermissionMode>,
    pub bus_vars: BusVars,
    pub usage_budget: Arc<UsageBudget>,
}

#[derive(Debug)]
pub(crate) enum Order {
    Send(Prompt),
    Interrupt,
    Model(ModelId),
    Effort(Effort),
    Mode(PermissionMode),
    Answer(Box<ToolAsk>, AskAnswer),
    Rewind(RewindRequest),
    Acp(AcpOrder),
}

#[derive(Debug)]
pub(crate) enum AcpOrder {
    Mode(AcpModeId),
    Option(AcpConfigOptionId, AcpConfigValue),
    SignIn(AcpAuthMethodId),
}

pub(crate) trait LiveHarness: Send {
    fn order(&self, order: Order);

    fn is_alive(&self) -> bool;

    fn stop(&self) {}

    fn run(&self, command: &SlashCommand) {
        match command {
            SlashCommand::Model(model) => self.order(Order::Model(model.clone())),
            SlashCommand::Effort(effort) => self.order(Order::Effort(*effort)),
            SlashCommand::Thinking(_) => {}
        }
    }
}

pub struct LiveChat {
    harness: Box<dyn LiveHarness>,
}

impl LiveChat {
    pub fn answer(&self, ask: &ToolAsk, answer: &AskAnswer) {
        self.harness
            .order(Order::Answer(Box::new(ask.clone()), answer.clone()));
    }

    pub fn interrupt(&self) {
        self.harness.order(Order::Interrupt);
    }

    pub fn set_model(&self, model: &ModelId) {
        self.harness.order(Order::Model(model.clone()));
    }

    pub fn set_permission_mode(&self, mode: PermissionMode) {
        self.harness.order(Order::Mode(mode));
    }

    pub fn set_acp_mode(&self, mode: AcpModeId) {
        self.harness.order(Order::Acp(AcpOrder::Mode(mode)));
    }

    pub fn set_acp_option(&self, option: AcpConfigOptionId, value: AcpConfigValue) {
        self.harness
            .order(Order::Acp(AcpOrder::Option(option, value)));
    }

    pub fn sign_in(&self, method: AcpAuthMethodId) {
        self.harness.order(Order::Acp(AcpOrder::SignIn(method)));
    }

    pub fn run(&self, command: &SlashCommand) {
        self.harness.run(command);
    }

    pub fn send(&self, prompt: impl Into<Prompt>) {
        self.harness.order(Order::Send(prompt.into()));
    }

    pub fn rewind(&self, request: RewindRequest) {
        self.harness.order(Order::Rewind(request));
    }

    #[must_use]
    pub fn is_alive(&self) -> bool {
        self.harness.is_alive()
    }

    pub fn stop(&self) {
        self.harness.stop();
    }
}

impl Drop for LiveChat {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Clone)]
pub enum ChatBackend {
    Claude(LiveConfig),
    Codex(LiveConfig),
    Opencode(Arc<OpencodeServer>, LiveConfig),
    Gateway(GatewayAccess, EngineConfig),
    Acp(AcpConfig),
}

/// # Errors
///
/// Fails when the harness cannot be started.
pub fn start_chat(
    backend: ChatBackend,
    session: &LiveSession,
    sink: impl FnMut(LiveEvent) + Send + 'static,
) -> Result<LiveChat, ChatError> {
    let harness: Box<dyn LiveHarness> = match backend {
        ChatBackend::Claude(config) => Box::new(spawn_claude(&config, session, sink)?),
        ChatBackend::Codex(config) => Box::new(spawn_codex(config, session, sink)),
        ChatBackend::Opencode(server, config) => {
            Box::new(spawn_opencode(server, config, session, sink))
        }
        ChatBackend::Gateway(access, config) => {
            Box::new(spawn_engine(access, config, session, sink))
        }
        ChatBackend::Acp(config) => Box::new(spawn_acp(config, session, sink)),
    };
    Ok(LiveChat { harness })
}

pub(crate) fn keep_stderr_tail(stderr: impl Read, tail: &Mutex<String>) {
    for line in BufReader::new(stderr).lines() {
        let Ok(line) = line else { break };
        let mut tail = tail.lock().unwrap_or_else(PoisonError::into_inner);
        tail.push_str(&line);
        tail.push('\n');
        trim_to_tail(&mut tail, STDERR_TAIL_CHARS);
    }
}

fn trim_to_tail(text: &mut String, chars: usize) {
    if let Some((cut, _)) = text.char_indices().rev().nth(chars) {
        text.drain(..cut);
    }
}

#[cfg(test)]
mod tests;
