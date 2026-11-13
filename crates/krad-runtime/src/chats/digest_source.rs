use krad_domain::{AgentPreset, ChatMessage, Effort, ModelId};
use krad_protocol::AgentRef;

use super::chat::Chat;
use super::Chats;

#[derive(Clone, Debug)]
pub(crate) struct DigestSource {
    pub(crate) chat: AgentRef,
    pub(crate) harness: AgentPreset,
    pub(crate) messages: Vec<ChatMessage>,
    pub(crate) model: Option<ModelId>,
    pub(crate) effort: Option<Effort>,
}

impl From<&Chat> for DigestSource {
    fn from(chat: &Chat) -> Self {
        Self {
            chat: chat.id.clone(),
            harness: chat.harness,
            messages: chat.messages.clone(),
            model: chat.choices.model.clone(),
            effort: chat.choices.effort,
        }
    }
}

impl Chats {
    pub(crate) fn open_chats(&self) -> impl Iterator<Item = (&AgentRef, bool)> {
        self.open
            .iter()
            .map(|(id, chat)| (id, chat.working || !chat.outbox.is_empty()))
    }

    pub(crate) fn is_asking(&self, id: &AgentRef) -> bool {
        self.open.get(id).is_some_and(|chat| !chat.asks.is_empty())
    }

    pub(crate) fn digest_source(&self, id: &AgentRef) -> Option<DigestSource> {
        self.open.get(id).map(DigestSource::from)
    }

    pub(crate) fn take_ended(&mut self) -> Vec<DigestSource> {
        std::mem::take(&mut self.ended)
    }
}
