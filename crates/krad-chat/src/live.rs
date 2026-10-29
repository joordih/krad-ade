use krad_domain::{ChatEvent, Effort, ModelId, PermissionMode, ResumeId};
use std::collections::hash_map::RandomState;
use std::collections::VecDeque;
use std::hash::{BuildHasher, Hash, Hasher};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::command::{live_chat_command, BusVars, LiveSession, Orchestration};
use crate::control::{
    answer_line, initialize_line, interrupt_line, route, set_model_line, set_permission_mode_line,
    Approvals, AskAnswer, LiveEvent, Routed, ToolAsk,
};
use crate::error::ChatError;
use crate::message::{user_message_line, SlashCommand};
use crate::parse::claude;
use crate::process::{harness_command, Runner};
use crate::turn::{closing_event, STDERR_TAIL_CHARS};

const UUID_VERSION_MASK: u128 = 0xffff_ffff_ffff_0fff_3fff_ffff_ffff_ffff;
const UUID_VERSION_BITS: u128 = 0x0000_0000_0000_4000_8000_0000_0000_0000;
const UUID_GROUP_ENDS: [usize; 4] = [8, 12, 16, 20];
const UUID_HEX_DIGITS: usize = 32;
const REQUEST_ID_PREFIX: &str = "krad_";
const MISSING_CONVERSATION_TEXT: &str = "No conversation found";

#[must_use]
pub fn is_missing_conversation(error: &str) -> bool {
    error.contains(MISSING_CONVERSATION_TEXT)
}

#[must_use]
pub fn new_session_id() -> ResumeId {
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
    ResumeId::try_from(uuid).unwrap_or_else(|_| unreachable!("a uuid is a valid resume id"))
}

#[derive(Debug)]
pub struct MessageQueue<T> {
    busy: bool,
    waiting: VecDeque<T>,
}

impl<T> Default for MessageQueue<T> {
    fn default() -> Self {
        Self {
            busy: false,
            waiting: VecDeque::new(),
        }
    }
}

impl<T> MessageQueue<T> {
    #[must_use]
    pub fn push(&mut self, message: T) -> Option<T> {
        if self.busy {
            self.waiting.push_back(message);
            return None;
        }
        self.busy = true;
        Some(message)
    }

    #[must_use]
    pub fn finish(&mut self) -> Option<T> {
        let next = self.waiting.pop_front();
        self.busy = next.is_some();
        next
    }

    pub fn clear(&mut self) {
        self.busy = false;
        self.waiting.clear();
    }

    #[must_use]
    pub const fn is_busy(&self) -> bool {
        self.busy
    }

    pub fn waiting(&self) -> impl Iterator<Item = &T> {
        self.waiting.iter()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveConfig {
    pub runner: Runner,
    pub model: Option<ModelId>,
    pub cwd: PathBuf,
    pub extra_dirs: Vec<PathBuf>,
    pub orchestration: Option<Orchestration>,
    pub system_prompt: Option<String>,
    pub effort: Option<Effort>,
    pub permission_mode: Option<PermissionMode>,
    pub bus_vars: BusVars,
    pub approvals: Approvals,
}

pub struct LiveChat {
    pub config: LiveConfig,
    outbox: Sender<String>,
    child: Arc<Mutex<Option<Child>>>,
    pending: Arc<AtomicUsize>,
    stopped: Arc<AtomicBool>,
    exited: Arc<AtomicBool>,
    next_request: AtomicU64,
}

impl LiveChat {
    fn request_id(&self) -> String {
        let id = self.next_request.fetch_add(1, Ordering::SeqCst);
        format!("{REQUEST_ID_PREFIX}{id}")
    }

    fn write(&self, line: String) {
        self.outbox.send(line).unwrap_or_default();
    }

    pub fn answer(&self, ask: &ToolAsk, answer: &AskAnswer) {
        self.write(answer_line(ask, answer));
    }

    pub fn interrupt(&self) {
        self.write(interrupt_line(&self.request_id()));
    }

    pub fn set_model(&self, model: &ModelId) {
        self.write(set_model_line(&self.request_id(), model));
    }

    pub fn set_permission_mode(&self, mode: PermissionMode) {
        self.write(set_permission_mode_line(&self.request_id(), mode));
    }

    pub fn run(&self, command: &SlashCommand) {
        self.send(&command.to_string());
    }

    pub fn send(&self, prompt: &str) {
        self.pending.fetch_add(1, Ordering::SeqCst);
        if self.outbox.send(user_message_line(prompt)).is_err() {
            self.pending.fetch_sub(1, Ordering::SeqCst);
        }
    }

    #[must_use]
    pub fn is_alive(&self) -> bool {
        !self.exited.load(Ordering::SeqCst)
    }

    pub fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        let mut child = self.child.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(child) = child.as_mut() {
            child.kill().unwrap_or_default();
        }
    }
}

impl Drop for LiveChat {
    fn drop(&mut self) {
        self.stop();
    }
}

/// # Errors
///
/// Fails when the harness cannot be started.
pub fn spawn_live_chat(
    config: LiveConfig,
    session: &LiveSession,
    mut sink: impl FnMut(LiveEvent) + Send + 'static,
) -> Result<LiveChat, ChatError> {
    let command = live_chat_command(&config, session);
    let mut env = command.env;
    env.extend(
        config
            .orchestration
            .iter()
            .flat_map(|orchestration| orchestration.env.0.iter().cloned()),
    );
    env.extend(config.bus_vars.0.iter().cloned());
    let mut child = harness_command(&config.runner, &config.cwd, &command.args, &env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdin = child.stdin.take();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (outbox, inbox) = channel();
    if let Some(stdin) = stdin {
        thread::spawn(move || write_messages(stdin, &inbox));
    }
    let stderr_tail = Arc::new(Mutex::new(String::new()));
    let stderr_reader = stderr.map(|stderr| {
        let tail = Arc::clone(&stderr_tail);
        thread::spawn(move || keep_stderr_tail(stderr, &tail))
    });
    let chat = LiveChat {
        config,
        outbox,
        child: Arc::new(Mutex::new(Some(child))),
        pending: Arc::new(AtomicUsize::new(0)),
        stopped: Arc::new(AtomicBool::new(false)),
        exited: Arc::new(AtomicBool::new(false)),
        next_request: AtomicU64::new(0),
    };
    let init_id = chat.request_id();
    if chat.config.approvals == Approvals::Asked {
        chat.write(initialize_line(&init_id));
    }
    let child = Arc::clone(&chat.child);
    let pending = Arc::clone(&chat.pending);
    let stopped = Arc::clone(&chat.stopped);
    let exited = Arc::clone(&chat.exited);
    thread::spawn(move || {
        if let Some(stdout) = stdout {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                route_line(&line, &init_id, &pending, &mut sink);
            }
        }
        if let Some(reader) = stderr_reader {
            reader.join().unwrap_or_default();
        }
        let taken = child.lock().unwrap_or_else(PoisonError::into_inner).take();
        let status = taken.and_then(|mut child| child.wait().ok());
        exited.store(true, Ordering::SeqCst);
        let stderr = stderr_tail
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(event) = closing_event(
            pending.load(Ordering::SeqCst) == 0,
            stopped.load(Ordering::SeqCst),
            status.and_then(|status| status.code()),
            status.is_some_and(|status| status.success()),
            &stderr,
        ) {
            sink(LiveEvent::Chat(event));
        }
    });
    Ok(chat)
}

fn route_line(line: &str, init_id: &str, pending: &AtomicUsize, sink: &mut impl FnMut(LiveEvent)) {
    let value = match route(line, init_id) {
        None | Some(Routed::Control) => return,
        Some(Routed::Ask(ask)) => return sink(LiveEvent::Ask(ask)),
        Some(Routed::Ready(catalog)) => return sink(LiveEvent::Ready(catalog)),
        Some(Routed::Conversation(value)) => value,
    };
    for event in claude(&value) {
        if matches!(event, ChatEvent::Done | ChatEvent::Error(_)) {
            pending
                .try_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
                    count.checked_sub(1)
                })
                .unwrap_or_default();
        }
        sink(LiveEvent::Chat(event));
    }
}

fn write_messages(mut stdin: ChildStdin, inbox: &Receiver<String>) {
    while let Ok(line) = inbox.recv() {
        if stdin
            .write_all(line.as_bytes())
            .and_then(|()| stdin.flush())
            .is_err()
        {
            break;
        }
    }
}

fn keep_stderr_tail(stderr: impl Read, tail: &Mutex<String>) {
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
