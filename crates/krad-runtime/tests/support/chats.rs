use futures::executor::block_on;
use futures::StreamExt as _;
use krad_chat::{BusVars, LiveConfig, LiveSession, Runner};
use krad_domain::{AgentPreset, EnvironmentName, Host, TerminalId};
use krad_host::HostLink;
use krad_protocol::{AgentRef, ChatMirror, ChatSummary, Command, EventKind, Feed, Reply, Scope};
use krad_runtime::{ChatPlace, ChatStart, Runtime, RuntimeHandle, RuntimeSetup};
use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

pub type TestResult = Result<(), Box<dyn Error>>;

pub const CLAUDE_HARNESS: &str = "../krad-chat/tests/fixtures/fake-harness/claude";
pub const ASK_HARNESS: &str = "tests/fixtures/fake-harness/claude-ask";
const WAIT: Duration = Duration::from_secs(10);
const TIMED_OUT: &str = "the awaited chat event never came";
const NOT_A_SNAPSHOT: &str = "the feed did not start from a snapshot";
const NO_CHAT: &str = "the runtime has no such chat";

pub fn fake_runner(path: &str) -> Runner {
    Runner::Native(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path))
}

pub fn chat(id: u64) -> AgentRef {
    AgentRef {
        environment: EnvironmentName::new("main".to_owned()),
        terminal: TerminalId(id),
    }
}

pub fn claude_start(runner: Runner, session: LiveSession) -> ChatStart {
    ChatStart {
        harness: AgentPreset::ClaudeCode,
        config: LiveConfig {
            runner,
            model: None,
            cwd: std::env::temp_dir(),
            system_prompt: None,
            effort: None,
            permission_mode: None,
            bus_vars: BusVars::default(),
            usage_budget: std::sync::Arc::default(),
        },
        session,
        place: ChatPlace {
            host: Host::Local,
            link: HostLink::Local,
            root: None,
        },
        messages: Vec::new(),
        queued: Vec::new(),
    }
}

pub fn start_runtime() -> Result<RuntimeHandle, Box<dyn Error>> {
    Ok(Runtime::start(RuntimeSetup::default())?)
}

pub trait Ask {
    fn send_now(&self, command: Command) -> Result<Reply, Box<dyn Error>>;
    fn chat_snapshot(&self, id: &AgentRef) -> Result<ChatSummary, Box<dyn Error>>;
}

impl Ask for RuntimeHandle {
    fn send_now(&self, command: Command) -> Result<Reply, Box<dyn Error>> {
        Ok(block_on(self.send(self.next_request(), command))?)
    }

    fn chat_snapshot(&self, id: &AgentRef) -> Result<ChatSummary, Box<dyn Error>> {
        block_on(self.snapshot(Scope::Runtime))?
            .chats
            .into_iter()
            .find(|summary| summary.tail.chat == *id)
            .ok_or_else(|| NO_CHAT.into())
    }
}

pub struct ChatFeed {
    feed: Receiver<Feed>,
    mirrors: HashMap<AgentRef, ChatMirror>,
    seen: Vec<EventKind>,
}

impl ChatFeed {
    pub fn follow(runtime: &RuntimeHandle) -> Result<Self, Box<dyn Error>> {
        let mut events = runtime.subscribe();
        let (sender, feed) = channel();
        std::thread::spawn(move || {
            while let Some(next) = block_on(events.next()) {
                if sender.send(next).is_err() {
                    break;
                }
            }
        });
        let Feed::Snapshot(snapshot) = feed.recv_timeout(WAIT)? else {
            return Err(NOT_A_SNAPSHOT.into());
        };
        let mirrors = snapshot
            .chats
            .into_iter()
            .map(|summary| (summary.tail.chat.clone(), ChatMirror::from(summary)))
            .collect();
        Ok(Self {
            feed,
            mirrors,
            seen: Vec::new(),
        })
    }

    fn take(&mut self, deadline: Instant) -> Result<EventKind, Box<dyn Error>> {
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if let Feed::Event(event) = self.feed.recv_timeout(left)? {
                self.mirror_event(&event.kind);
                self.seen.push(event.kind.clone());
                return Ok(event.kind);
            }
        }
    }

    fn mirror_event(&mut self, kind: &EventKind) {
        let chat = match kind {
            EventKind::ChatOpened { chat, state } => {
                self.mirrors
                    .insert(chat.clone(), ChatMirror::new((**state).clone()));
                return;
            }
            EventKind::ChatClosed { chat } => {
                self.mirrors.remove(chat);
                return;
            }
            kind => match kind.chat() {
                Some(chat) => chat,
                None => return,
            },
        };
        if let Some(mirror) = self.mirrors.get_mut(chat) {
            mirror.apply(kind);
        }
    }

    pub fn until(
        &mut self,
        wanted: impl Fn(&EventKind) -> bool,
    ) -> Result<EventKind, Box<dyn Error>> {
        let deadline = Instant::now() + WAIT;
        loop {
            let kind = self.take(deadline).map_err(|_| TIMED_OUT)?;
            if wanted(&kind) {
                return Ok(kind);
            }
        }
    }

    pub fn until_chat(
        &mut self,
        id: &AgentRef,
        wanted: impl Fn(&ChatMirror) -> bool,
    ) -> Result<(), Box<dyn Error>> {
        let deadline = Instant::now() + WAIT;
        while !self.mirrors.get(id).is_some_and(&wanted) {
            self.take(deadline).map_err(|_| TIMED_OUT)?;
        }
        Ok(())
    }

    pub fn mirror(&self, id: &AgentRef) -> Result<ChatMirror, Box<dyn Error>> {
        self.mirrors.get(id).cloned().ok_or_else(|| NO_CHAT.into())
    }

    pub fn saw(&self, wanted: impl Fn(&EventKind) -> bool) -> bool {
        self.seen.iter().any(wanted)
    }
}
