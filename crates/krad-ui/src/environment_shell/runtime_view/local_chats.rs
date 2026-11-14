use krad_protocol::{AgentRef, ChatMirror, ChatSummary, EventKind};
use std::collections::HashMap;

#[derive(Clone, Default)]
pub(in crate::environment_shell) struct LocalChats(HashMap<AgentRef, ChatMirror>);

impl LocalChats {
    pub(in crate::environment_shell) fn get(&self, chat: &AgentRef) -> Option<&ChatMirror> {
        self.0.get(chat)
    }

    pub(in crate::environment_shell) fn contains(&self, chat: &AgentRef) -> bool {
        self.0.contains_key(chat)
    }

    pub(in crate::environment_shell) fn opening(&mut self, chat: AgentRef, mirror: ChatMirror) {
        self.0.entry(chat).or_insert(mirror);
    }

    pub(in crate::environment_shell) fn forget(&mut self, chat: &AgentRef) {
        self.0.remove(chat);
    }

    pub(super) fn replace(&mut self, summaries: &[ChatSummary]) {
        self.0 = summaries
            .iter()
            .map(|summary| (summary.tail.chat.clone(), summary.clone().into()))
            .collect();
    }

    pub(super) fn heard(&mut self, kind: &EventKind) {
        let Some(chat) = kind.chat() else {
            return;
        };
        match kind {
            EventKind::ChatClosed { .. } => {
                self.0.remove(chat);
            }
            EventKind::ChatOpened { state, .. } => {
                self.0
                    .entry(chat.clone())
                    .or_insert_with(|| ChatMirror::new((**state).clone()))
                    .apply(kind);
            }
            kind => {
                if let Some(mirror) = self.0.get_mut(chat) {
                    mirror.apply(kind);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{AgentPreset, EnvironmentName, Prompt, TerminalId};
    use krad_protocol::ChatState;

    fn chat() -> AgentRef {
        AgentRef {
            environment: EnvironmentName::new("main".to_owned()),
            terminal: TerminalId(4),
        }
    }

    #[test]
    fn an_opening_chat_shows_its_queue_until_the_runtime_tells_its_state_and_leaves_when_closed() {
        let mut chats = LocalChats::default();
        let mut opening = ChatState::new(AgentPreset::Codex);
        opening.queued = vec![Prompt::from("first")];
        chats.opening(chat(), ChatMirror::new(opening.clone()));
        chats.opening(chat(), ChatMirror::new(ChatState::new(AgentPreset::Codex)));
        assert_eq!(
            chats.get(&chat()).map(|mirror| mirror.state.queued.len()),
            Some(1),
            "a second opening keeps the first"
        );
        let told = ChatState::new(AgentPreset::Codex);
        chats.heard(&EventKind::ChatState {
            chat: chat(),
            state: Box::new(told.clone()),
        });
        assert_eq!(chats.get(&chat()).map(|mirror| &mirror.state), Some(&told));
        chats.heard(&EventKind::ChatClosed { chat: chat() });
        assert!(!chats.contains(&chat()));
        chats.heard(&EventKind::ChatState {
            chat: chat(),
            state: Box::new(opening),
        });
        assert!(!chats.contains(&chat()), "only an opened chat is kept");
        chats.heard(&EventKind::ChatOpened {
            chat: chat(),
            state: Box::new(told),
        });
        assert!(chats.contains(&chat()));
    }
}
