use futures::executor::block_on;
use futures::StreamExt as _;
use krad_protocol::{AgentRef, ChatMirror, ChatSummary, EventKind, Feed, Scope};
use krad_runtime::RuntimeHandle;
use std::error::Error;
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

const WAIT: Duration = Duration::from_secs(30);
const TIMED_OUT: &str = "the awaited chat event never came";
const NO_CHAT: &str = "the chat is not open";

pub struct ChatFeed {
    runtime: RuntimeHandle,
    feed: Receiver<Feed>,
    mirrors: std::collections::HashMap<AgentRef, ChatMirror>,
}

impl ChatFeed {
    pub fn follow(runtime: &RuntimeHandle) -> Self {
        let mut events = runtime.subscribe();
        let (sender, feed) = channel();
        std::thread::spawn(move || {
            while let Some(next) = block_on(events.next()) {
                if sender.send(next).is_err() {
                    break;
                }
            }
        });
        Self {
            runtime: runtime.clone(),
            feed,
            mirrors: std::collections::HashMap::new(),
        }
    }

    fn take(&mut self, deadline: Instant) -> Result<EventKind, Box<dyn Error>> {
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let Feed::Event(event) = self.feed.recv_timeout(left).map_err(|_| TIMED_OUT)? else {
                continue;
            };
            match &event.kind {
                EventKind::ChatOpened { chat, state } => {
                    let mirror = self
                        .summary(chat)?
                        .map_or_else(|| ChatMirror::new((**state).clone()), ChatMirror::from);
                    self.mirrors.insert(chat.clone(), mirror);
                }
                EventKind::ChatClosed { chat } => {
                    self.mirrors.remove(chat);
                }
                kind => {
                    if let Some(mirror) = kind.chat().and_then(|chat| self.mirrors.get_mut(chat)) {
                        mirror.apply(kind);
                    }
                }
            }
            return Ok(event.kind);
        }
    }

    fn summary(&self, chat: &AgentRef) -> Result<Option<ChatSummary>, Box<dyn Error>> {
        Ok(block_on(self.runtime.snapshot(Scope::Runtime))?
            .chats
            .into_iter()
            .find(|summary| summary.tail.chat == *chat))
    }

    pub fn until(
        &mut self,
        wanted: impl Fn(&EventKind) -> bool,
    ) -> Result<EventKind, Box<dyn Error>> {
        let deadline = Instant::now() + WAIT;
        loop {
            let kind = self.take(deadline)?;
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
            self.take(deadline)?;
        }
        Ok(())
    }

    pub fn mirror(&self, id: &AgentRef) -> Result<ChatMirror, Box<dyn Error>> {
        self.mirrors.get(id).cloned().ok_or_else(|| NO_CHAT.into())
    }
}
