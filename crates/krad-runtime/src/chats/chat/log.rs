use krad_domain::{ChatLogChange, ChatMessage};
use krad_protocol::EventKind;

use super::Chat;

impl Chat {
    pub(in crate::chats) fn prepend(
        &mut self,
        mut earlier: Vec<ChatMessage>,
        outbox: &mut Vec<EventKind>,
    ) {
        let count = earlier.len();
        if count == 0 {
            return;
        }
        earlier.append(&mut self.messages);
        self.messages = earlier;
        if let Some(rewinding) = self.rewinding.as_mut() {
            rewinding.index += count;
        }
        outbox.push(EventKind::ChatMessagesPrepended {
            chat: self.id.clone(),
            count,
        });
    }

    pub(in crate::chats) fn replace_messages(
        &mut self,
        messages: Vec<ChatMessage>,
        outbox: &mut Vec<EventKind>,
    ) {
        let before = self.messages.len();
        self.messages = messages;
        if before > 0 {
            outbox.push(EventKind::ChatMessagesCut {
                chat: self.id.clone(),
                len: 0,
            });
        }
        self.tell_messages(0, Some(0), outbox);
    }

    pub(in crate::chats) fn cut(&mut self, len: usize, outbox: &mut Vec<EventKind>) {
        let before = self.messages.len();
        self.messages.truncate(len);
        self.tell_messages(before, None, outbox);
    }

    pub(super) fn tell_messages(
        &self,
        before: usize,
        touched: Option<usize>,
        outbox: &mut Vec<EventKind>,
    ) {
        self.tell_changes(
            ChatLogChange::since(&self.messages, before, touched),
            outbox,
        );
    }

    pub(super) fn tell_changes(&self, changes: Vec<ChatLogChange>, outbox: &mut Vec<EventKind>) {
        for change in changes {
            outbox.push(match change {
                ChatLogChange::Added { index, message } => EventKind::ChatMessageAdded {
                    chat: self.id.clone(),
                    index,
                    message: Box::new(message),
                },
                ChatLogChange::Changed { index, message } => EventKind::ChatMessageChanged {
                    chat: self.id.clone(),
                    index,
                    message: Box::new(message),
                },
                ChatLogChange::Appended { index, text } => EventKind::ChatMessageAppended {
                    chat: self.id.clone(),
                    index,
                    text,
                },
                ChatLogChange::Cut { len } => EventKind::ChatMessagesCut {
                    chat: self.id.clone(),
                    len,
                },
            });
        }
    }
}
