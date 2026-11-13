use serde::{Deserialize, Serialize};
use std::time::SystemTime;

use crate::chat::ChatEntry;
use crate::harness::is_worker_brief;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatOpener {
    Lead,
    User,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChatOrigin {
    pub opener: ChatOpener,
    pub started: Option<SystemTime>,
}

impl ChatOrigin {
    #[must_use]
    pub fn of(entries: &[ChatEntry], started: Option<SystemTime>) -> Option<Self> {
        let first = entries.iter().find_map(|entry| match entry {
            ChatEntry::User { text, .. } => Some(text),
            ChatEntry::Event(_) | ChatEntry::Asked(_) => None,
        })?;
        let opener = if is_worker_brief(first) {
            ChatOpener::Lead
        } else {
            ChatOpener::User
        };
        Some(Self { opener, started })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentPreset;
    use crate::chat::ChatEvent;

    #[test]
    fn a_conversation_a_worker_brief_opened_comes_from_the_lead() {
        let user = |text: String| ChatEntry::User {
            text,
            preset: AgentPreset::ClaudeCode,
            model: None,
            images: Vec::new(),
        };
        let brief = crate::harness::chat_worker_brief("Verify the signature");
        let origin = ChatOrigin::of(&[ChatEntry::Event(ChatEvent::Done), user(brief)], None);
        assert_eq!(origin.map(|origin| origin.opener), Some(ChatOpener::Lead));
        let asked = ChatOrigin::of(&[user("Why?".to_owned())], None);
        assert_eq!(asked.map(|origin| origin.opener), Some(ChatOpener::User));
        assert_eq!(ChatOrigin::of(&[], None), None);
    }
}
