use krad_domain::{ChatMessage, PermissionMode, Prompt, ResumeId, Speaker};
use krad_protocol::{AgentRef, ChatLaunch, EventKind, Reply};

use super::{asks_of, Chat, ChatWork, Chats};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Launch {
    Pending,
    Started,
    Failed(String),
}

fn launch_of(message: &ChatMessage) -> Launch {
    match (&message.speaker, &message.error) {
        (Speaker::Assistant, Some(error)) => Launch::Failed(error.clone()),
        (Speaker::Assistant, None) => Launch::Started,
        (Speaker::User, _) => Launch::Pending,
    }
}

impl Chat {
    fn launch(&self) -> Launch {
        if !self.asks.is_empty() {
            return Launch::Started;
        }
        self.messages
            .iter()
            .map(launch_of)
            .find(|launch| *launch != Launch::Pending)
            .unwrap_or(Launch::Pending)
    }

    pub(super) fn shown_launch(&self) -> ChatLaunch {
        let asked = self.working || !self.outbox.is_empty();
        if asked && self.launch() == Launch::Pending {
            ChatLaunch::Starting
        } else {
            ChatLaunch::Answered
        }
    }
}

impl Chats {
    pub(crate) fn is_open(&self, id: &AgentRef) -> bool {
        self.open.contains_key(id)
    }

    pub(crate) fn rejoin(&self, id: &AgentRef, outbox: &mut Vec<EventKind>) -> bool {
        let Some(chat) = self.open.get(id) else {
            return false;
        };
        outbox.push(EventKind::ChatOpened {
            chat: id.clone(),
            state: Box::new(chat.state()),
        });
        outbox.extend(chat.messages.iter().enumerate().map(|(index, message)| {
            EventKind::ChatMessageAdded {
                chat: id.clone(),
                index,
                message: Box::new(message.clone()),
            }
        }));
        outbox.extend(asks_of(chat).into_iter().map(EventKind::ChatAsk));
        true
    }

    pub(crate) fn is_busy(&self, id: &AgentRef) -> bool {
        self.open
            .get(id)
            .is_some_and(|chat| chat.working || !chat.outbox.is_empty() || !chat.asks.is_empty())
    }

    pub(crate) fn permission_mode(&self, id: &AgentRef) -> Option<PermissionMode> {
        self.open.get(id)?.choices.permission_mode
    }

    pub(crate) fn launch(&self, id: &AgentRef) -> Launch {
        self.open
            .get(id)
            .map_or_else(|| Launch::Failed(String::new()), Chat::launch)
    }

    pub(crate) fn send_prompt(
        &mut self,
        id: &AgentRef,
        prompt: Prompt,
        work: &mut ChatWork<'_>,
    ) -> Reply {
        let reply = self.queue_prompt(id, prompt);
        self.flush(id, work);
        reply
    }

    pub(crate) fn stop_turn(&mut self, id: &AgentRef) {
        drop(self.interrupt(id));
    }

    pub(crate) fn resumes(&self) -> impl Iterator<Item = (&AgentRef, &ResumeId)> {
        self.open
            .iter()
            .filter_map(|(id, chat)| Some((id, chat.resume.as_ref()?)))
    }

    pub(crate) fn tail(&self, id: &AgentRef) -> Option<&[ChatMessage]> {
        self.open.get(id).map(|chat| chat.messages.as_slice())
    }
}
