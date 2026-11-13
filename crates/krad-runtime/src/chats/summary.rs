use krad_protocol::{AgentRef, ChatAsk, ChatPage, ChatSummary};

use super::chat::Chat;
use super::{Chats, CHAT_PAGE};

impl Chats {
    pub(crate) fn snapshot(&self) -> Vec<ChatSummary> {
        let mut summaries: Vec<ChatSummary> = self
            .open
            .values()
            .map(|chat| ChatSummary {
                state: chat.state(),
                tail: page(chat, chat.messages.len(), CHAT_PAGE),
                asks: asks_of(chat),
            })
            .collect();
        summaries.sort_by(|left, right| {
            let key = |summary: &ChatSummary| {
                (
                    summary.tail.chat.environment.as_str().to_owned(),
                    summary.tail.chat.terminal.0,
                )
            };
            key(left).cmp(&key(right))
        });
        summaries
    }

    pub(crate) fn page(&self, id: &AgentRef, before: usize, count: usize) -> Option<ChatPage> {
        self.open.get(id).map(|chat| page(chat, before, count))
    }
}

fn page(chat: &Chat, before: usize, count: usize) -> ChatPage {
    let end = before.min(chat.messages.len());
    let from = end.saturating_sub(count);
    ChatPage {
        chat: chat.id.clone(),
        len: chat.messages.len(),
        from,
        messages: chat.messages[from..end].to_vec(),
    }
}

pub(super) fn asks_of(chat: &Chat) -> Vec<ChatAsk> {
    chat.asks
        .iter()
        .map(|(ask, held)| ChatAsk {
            chat: chat.id.clone(),
            ask: *ask,
            kind: held.kind.clone(),
            can_allow_always: held.can_allow_always(),
        })
        .collect()
}
