mod claude;

use krad_domain::{Effort, ModelId, PermissionMode, Prompt, ResumeId};
use std::collections::hash_map::RandomState;
use std::fmt;
use std::hash::{BuildHasher, Hash, Hasher};
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::codex::spawn_codex;
use crate::control::{AskAnswer, LiveEvent, ToolAsk};
use crate::engine::{spawn_engine, EngineConfig};
use crate::error::ChatError;
use crate::message::SlashCommand;
use crate::opencode::{spawn_opencode, OpencodeServer};
use crate::process::Runner;
use crate::rewind::{ResumeCut, RewindRequest};
use claude::spawn_claude;

const STDERR_TAIL_CHARS: usize = 600;
const UUID_VERSION_MASK: u128 = 0xffff_ffff_ffff_0fff_3fff_ffff_ffff_ffff;
const UUID_VERSION_BITS: u128 = 0x0000_0000_0000_4000_8000_0000_0000_0000;
const UUID_GROUP_ENDS: [usize; 4] = [8, 12, 16, 20];
const UUID_HEX_DIGITS: usize = 32;
#[must_use]
pub fn new_session_id() -> ResumeId {
    ResumeId::try_from(new_uuid()).unwrap_or_else(|_| unreachable!("a uuid is a valid resume id"))
}

pub(crate) fn new_uuid() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or_default(|elapsed| elapsed.as_nanos());
    let halves = [RandomState::new(), RandomState::new()].map(|state| {
        let mut hasher = state.build_hasher();
        std::process::id().hash(&mut hasher);
        nanos.hash(&mut hasher);
        u128::from(hasher.finish())
    });
    let bits = ((halves[0] << u64::BITS) | halves[1]) & UUID_VERSION_MASK | UUID_VERSION_BITS;
    let hex = format!("{bits:0UUID_HEX_DIGITS$x}");
    let mut uuid = String::with_capacity(UUID_HEX_DIGITS + UUID_GROUP_ENDS.len());
    for (at, digit) in hex.chars().enumerate() {
        if UUID_GROUP_ENDS.contains(&at) {
            uuid.push('-');
        }
        uuid.push(digit);
    }
    uuid
}

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveSession {
    Start(ResumeId),
    Resume(ResumeId),
    Fork(ResumeId),
    Cut(ResumeCut),
}

impl LiveSession {
    #[must_use]
    pub const fn history(&self) -> Option<&ResumeId> {
        match self {
            Self::Start(_) => None,
            Self::Resume(id) | Self::Fork(id) | Self::Cut(ResumeCut { from: id, .. }) => Some(id),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveConfig {
    pub runner: Runner,
    pub model: Option<ModelId>,
    pub cwd: PathBuf,
    pub system_prompt: Option<String>,
    pub effort: Option<Effort>,
    pub permission_mode: Option<PermissionMode>,
    pub bus_vars: BusVars,
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

pub enum ChatBackend {
    Claude(LiveConfig),
    Codex(LiveConfig),
    Opencode(Arc<OpencodeServer>, LiveConfig),
    Gateway(EngineConfig),
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
        ChatBackend::Gateway(config) => Box::new(spawn_engine(config, session, sink)),
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
