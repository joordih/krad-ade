mod claude;

use krad_domain::{Effort, ModelId, PermissionMode, Prompt, ResumeId};
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hash, Hasher};
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::codex::{spawn_codex, CodexChat, Order as CodexOrder};
use crate::command::{BusVars, LiveSession};
use crate::control::{
    answer_line, interrupt_line, set_model_line, set_permission_mode_line, AskAnswer, LiveEvent,
    ToolAsk,
};
use crate::engine::{spawn_engine, EngineChat, EngineConfig, Order as NativeOrder};
use crate::error::ChatError;
use crate::exit::STDERR_TAIL_CHARS;
use crate::message::SlashCommand;
use crate::opencode::{spawn_opencode, OpencodeChat, OpencodeServer, Order};
use crate::process::Runner;
use crate::rewind::RewindRequest;
use claude::{spawn_claude, ClaudeProcess};

const UUID_VERSION_MASK: u128 = 0xffff_ffff_ffff_0fff_3fff_ffff_ffff_ffff;
const UUID_VERSION_BITS: u128 = 0x0000_0000_0000_4000_8000_0000_0000_0000;
const UUID_GROUP_ENDS: [usize; 4] = [8, 12, 16, 20];
const UUID_HEX_DIGITS: usize = 32;
const MISSING_CONVERSATION_TEXT: &str = "No conversation found";

#[must_use]
pub fn is_missing_conversation(error: &str) -> bool {
    error.contains(MISSING_CONVERSATION_TEXT)
}

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

enum Engine {
    Claude(ClaudeProcess),
    Opencode(OpencodeChat),
    Codex(CodexChat),
    Native(EngineChat),
}

pub struct LiveChat {
    engine: Engine,
}

impl LiveChat {
    pub fn answer(&self, ask: &ToolAsk, answer: &AskAnswer) {
        match &self.engine {
            Engine::Claude(claude) => claude.write(answer_line(ask, answer)),
            Engine::Opencode(opencode) => {
                opencode.order(Order::Answer(Box::new(ask.clone()), answer.clone()));
            }
            Engine::Codex(codex) => {
                codex.order(CodexOrder::Answer(Box::new(ask.clone()), answer.clone()));
            }
            Engine::Native(native) => {
                native.order(NativeOrder::Answer(Box::new(ask.clone()), answer.clone()));
            }
        }
    }

    pub fn interrupt(&self) {
        match &self.engine {
            Engine::Claude(claude) => claude.write(interrupt_line(&claude.request_id())),
            Engine::Opencode(opencode) => opencode.order(Order::Interrupt),
            Engine::Codex(codex) => codex.order(CodexOrder::Interrupt),
            Engine::Native(native) => native.order(NativeOrder::Interrupt),
        }
    }

    pub fn set_model(&self, model: &ModelId) {
        match &self.engine {
            Engine::Claude(claude) => claude.write(set_model_line(&claude.request_id(), model)),
            Engine::Opencode(opencode) => opencode.order(Order::Model(model.clone())),
            Engine::Codex(codex) => codex.order(CodexOrder::Model(model.clone())),
            Engine::Native(native) => native.order(NativeOrder::Model(model.clone())),
        }
    }

    pub fn set_permission_mode(&self, mode: PermissionMode) {
        match &self.engine {
            Engine::Claude(claude) => {
                claude.write(set_permission_mode_line(&claude.request_id(), mode));
            }
            Engine::Opencode(opencode) => opencode.order(Order::Mode(mode)),
            Engine::Codex(codex) => codex.order(CodexOrder::Mode(mode)),
            Engine::Native(native) => native.order(NativeOrder::Mode(mode)),
        }
    }

    pub fn run(&self, command: &SlashCommand) {
        match (&self.engine, command) {
            (Engine::Claude(claude), command) => claude.run(Prompt::from(command.to_string())),
            (Engine::Opencode(opencode), SlashCommand::Model(model)) => {
                opencode.order(Order::Model(model.clone()));
            }
            (Engine::Opencode(opencode), SlashCommand::Effort(effort)) => {
                opencode.order(Order::Effort(*effort));
            }
            (Engine::Codex(codex), SlashCommand::Model(model)) => {
                codex.order(CodexOrder::Model(model.clone()));
            }
            (Engine::Codex(codex), SlashCommand::Effort(effort)) => {
                codex.order(CodexOrder::Effort(*effort));
            }
            (Engine::Native(native), SlashCommand::Model(model)) => {
                native.order(NativeOrder::Model(model.clone()));
            }
            (Engine::Native(native), SlashCommand::Effort(effort)) => {
                native.order(NativeOrder::Effort(*effort));
            }
            (
                Engine::Opencode(_) | Engine::Codex(_) | Engine::Native(_),
                SlashCommand::Thinking(_),
            ) => {}
        }
    }

    pub fn send(&self, prompt: impl Into<Prompt>) {
        let prompt = prompt.into();
        match &self.engine {
            Engine::Claude(claude) => claude.send(prompt),
            Engine::Opencode(opencode) => opencode.order(Order::Send(prompt)),
            Engine::Codex(codex) => codex.order(CodexOrder::Send(prompt)),
            Engine::Native(native) => native.order(NativeOrder::Send(prompt)),
        }
    }

    pub fn rewind(&self, request: RewindRequest) {
        match &self.engine {
            Engine::Claude(claude) => claude.rewind(request),
            Engine::Opencode(opencode) => opencode.order(Order::Rewind(request)),
            Engine::Codex(codex) => codex.order(CodexOrder::Rewind(request)),
            Engine::Native(native) => native.order(NativeOrder::Rewind(request)),
        }
    }

    #[must_use]
    pub fn is_alive(&self) -> bool {
        match &self.engine {
            Engine::Claude(claude) => !claude.exited.load(Ordering::SeqCst),
            Engine::Opencode(opencode) => opencode.is_alive(),
            Engine::Codex(codex) => codex.is_alive(),
            Engine::Native(native) => native.is_alive(),
        }
    }

    pub fn stop(&self) {
        match &self.engine {
            Engine::Claude(claude) => claude.stop(),
            Engine::Codex(codex) => codex.stop(),
            Engine::Native(native) => native.stop(),
            Engine::Opencode(_) => {}
        }
    }
}

impl Drop for LiveChat {
    fn drop(&mut self) {
        self.stop();
    }
}

#[must_use]
pub fn spawn_codex_chat(
    config: LiveConfig,
    session: &LiveSession,
    sink: impl FnMut(LiveEvent) + Send + 'static,
) -> LiveChat {
    LiveChat {
        engine: Engine::Codex(spawn_codex(config, session, sink)),
    }
}

#[must_use]
pub fn spawn_opencode_chat(
    server: Arc<OpencodeServer>,
    config: LiveConfig,
    session: &LiveSession,
    sink: impl FnMut(LiveEvent) + Send + 'static,
) -> LiveChat {
    LiveChat {
        engine: Engine::Opencode(spawn_opencode(server, config, session, sink)),
    }
}

#[must_use]
pub fn spawn_engine_chat(
    config: EngineConfig,
    session: &LiveSession,
    sink: impl FnMut(LiveEvent) + Send + 'static,
) -> LiveChat {
    LiveChat {
        engine: Engine::Native(spawn_engine(config, session, sink)),
    }
}

/// # Errors
///
/// Fails when the harness cannot be started.
pub fn spawn_live_chat(
    config: &LiveConfig,
    session: &LiveSession,
    sink: impl FnMut(LiveEvent) + Send + 'static,
) -> Result<LiveChat, ChatError> {
    Ok(LiveChat {
        engine: Engine::Claude(spawn_claude(config, session, sink)?),
    })
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
