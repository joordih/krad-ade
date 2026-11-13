use krad_domain::{ChatLogChange, ChatMessage};

use super::{ChatAsk, ChatPage, ChatState, ChatSummary};
use crate::event::EventKind;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MirrorChange {
    Message(usize),
    Cut(usize),
    Prepended(usize),
    State,
    Asks,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ChatMirror {
    pub state: ChatState,
    pub from: usize,
    pub messages: Vec<ChatMessage>,
    pub asks: Vec<ChatAsk>,
}

impl From<ChatSummary> for ChatMirror {
    fn from(summary: ChatSummary) -> Self {
        Self {
            state: summary.state,
            from: summary.tail.from,
            messages: summary.tail.messages,
            asks: summary.asks,
        }
    }
}

impl ChatMirror {
    #[must_use]
    pub const fn new(state: ChatState) -> Self {
        Self {
            state,
            from: 0,
            messages: Vec::new(),
            asks: Vec::new(),
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.from + self.messages.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn apply(&mut self, event: &EventKind) -> Option<MirrorChange> {
        match event {
            EventKind::ChatMessageAdded { index, message, .. }
            | EventKind::ChatMessageChanged { index, message, .. } => {
                self.put(*index, message)?;
                Some(MirrorChange::Message(*index))
            }
            EventKind::ChatMessageAppended { index, text, .. } => {
                let local = index.checked_sub(self.from)?;
                ChatLogChange::Appended {
                    index: local,
                    text: text.clone(),
                }
                .apply(&mut self.messages);
                Some(MirrorChange::Message(*index))
            }
            EventKind::ChatMessagesPrepended { count, .. } => {
                self.from += count;
                Some(MirrorChange::Prepended(*count))
            }
            EventKind::ChatMessagesCut { len, .. } => {
                self.cut(*len);
                Some(MirrorChange::Cut(*len))
            }
            EventKind::ChatOpened { state, .. } | EventKind::ChatState { state, .. } => {
                self.state.clone_from(state);
                Some(MirrorChange::State)
            }
            EventKind::ChatAsk(ask) => {
                self.asks.retain(|held| held.ask != ask.ask);
                self.asks.push(ask.clone());
                Some(MirrorChange::Asks)
            }
            EventKind::ChatAskSettled { ask, .. } => {
                let before = self.asks.len();
                self.asks.retain(|held| held.ask != *ask);
                (self.asks.len() != before).then_some(MirrorChange::Asks)
            }
            _ => None,
        }
    }

    pub fn prepend(&mut self, page: ChatPage) {
        let end = page.from + page.messages.len();
        if end != self.from {
            return;
        }
        let mut messages = page.messages;
        messages.append(&mut self.messages);
        self.messages = messages;
        self.from = page.from;
    }

    fn put(&mut self, index: usize, message: &ChatMessage) -> Option<()> {
        let local = index.checked_sub(self.from)?;
        if let Some(held) = self.messages.get_mut(local) {
            held.clone_from(message);
            return Some(());
        }
        (local == self.messages.len()).then(|| self.messages.push(message.clone()))
    }

    fn cut(&mut self, len: usize) {
        if let Some(local) = len.checked_sub(self.from) {
            self.messages.truncate(local);
            return;
        }
        self.messages.clear();
        self.from = len;
    }
}
