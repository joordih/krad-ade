use krad_domain::{EnvironmentData, TerminalId};
use krad_protocol::{
    hosted_summaries, AgentRef, ChatAsk, ChatFacts, ChatPage, ChatState, ChatSummary, HostedSummary,
};
use std::collections::HashMap;

use super::chat::Chat;
use super::{Chats, CHAT_PAGE};
use crate::runners::OwnActivity;

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

    pub(crate) fn hosted(
        &self,
        data: &EnvironmentData,
        activity: &OwnActivity,
    ) -> Vec<HostedSummary> {
        let states: HashMap<TerminalId, (ChatState, usize, &AgentRef)> = self
            .open
            .values()
            .filter(|chat| chat.id.environment == data.name)
            .map(|chat| (chat.id.terminal, (chat.state(), chat.asks.len(), &chat.id)))
            .collect();
        hosted_summaries(data, |terminal| {
            let (state, asks, chat) = states.get(&terminal)?;
            let heard = activity.of(chat);
            Some(ChatFacts {
                state,
                asks: *asks,
                last_activity: heard.map(|(_, at)| at),
                seq: heard.map(|(seq, _)| seq),
            })
        })
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
