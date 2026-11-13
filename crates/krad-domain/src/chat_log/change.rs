use serde::{Deserialize, Serialize};

use super::ChatMessage;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "change", rename_all = "snake_case")]
pub enum ChatLogChange {
    Added { index: usize, message: ChatMessage },
    Changed { index: usize, message: ChatMessage },
    Cut { len: usize },
}

impl ChatLogChange {
    #[must_use]
    pub fn since(messages: &[ChatMessage], before: usize, touched: Option<usize>) -> Vec<Self> {
        let mut changes = Vec::new();
        if messages.len() < before {
            changes.push(Self::Cut {
                len: messages.len(),
            });
        }
        let kept = before.min(messages.len());
        let first = touched.unwrap_or(kept).min(kept);
        for (index, message) in messages.iter().enumerate().skip(first) {
            let message = message.clone();
            changes.push(if index < kept {
                Self::Changed { index, message }
            } else {
                Self::Added { index, message }
            });
        }
        changes
    }

    pub fn apply(self, messages: &mut Vec<ChatMessage>) {
        match self {
            Self::Added { index, message } | Self::Changed { index, message } => {
                if index < messages.len() {
                    messages[index] = message;
                } else if index == messages.len() {
                    messages.push(message);
                }
            }
            Self::Cut { len } => messages.truncate(len),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentPreset;
    use crate::chat::{ChatEntry, ChatEvent};
    use crate::chat_log::fold_entry;

    fn user(text: &str) -> ChatEntry {
        ChatEntry::User {
            text: text.to_owned(),
            preset: AgentPreset::ClaudeCode,
            model: None,
            images: Vec::new(),
        }
    }

    fn replay(log: &mut Vec<ChatMessage>, copy: &mut Vec<ChatMessage>, entry: &ChatEntry) -> usize {
        let before = log.len();
        let touched = fold_entry(log, entry);
        let changes = ChatLogChange::since(log, before, touched);
        let count = changes.len();
        for change in changes {
            change.apply(copy);
        }
        count
    }

    #[test]
    fn folding_reports_only_the_messages_it_touched_and_a_copy_follows() {
        let mut log = Vec::new();
        let mut copy = Vec::new();
        assert_eq!(replay(&mut log, &mut copy, &user("one")), 2);
        assert_eq!(
            replay(
                &mut log,
                &mut copy,
                &ChatEntry::Event(ChatEvent::Text("a".to_owned()))
            ),
            1
        );
        assert_eq!(
            replay(&mut log, &mut copy, &ChatEntry::Event(ChatEvent::Done)),
            1
        );
        assert_eq!(replay(&mut log, &mut copy, &user("two")), 3);
        assert_eq!(
            replay(
                &mut log,
                &mut copy,
                &ChatEntry::Event(ChatEvent::Model("opus".to_owned()))
            ),
            0
        );
        assert_eq!(copy, log);
    }

    #[test]
    fn a_cut_log_tells_its_new_length() {
        let mut log = Vec::new();
        let mut copy = Vec::new();
        replay(&mut log, &mut copy, &user("one"));
        replay(&mut log, &mut copy, &user("two"));
        let before = log.len();
        log.truncate(2);
        for change in ChatLogChange::since(&log, before, None) {
            change.apply(&mut copy);
        }
        assert_eq!(copy, log);
    }
}
