use crate::protocol::Message;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Default)]
pub struct Mailbox {
    next_id: u64,
    inbox: HashMap<u64, Vec<Message>>,
    notified: HashSet<u64>,
}

impl Mailbox {
    pub fn post(&mut self, from: u64, from_title: String, to: u64, body: String) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.inbox.entry(to).or_default().push(Message {
            id,
            from,
            from_title,
            body,
        });
        self.notified.remove(&to);
        id
    }

    pub fn take(&mut self, terminal: u64) -> Vec<Message> {
        self.notified.remove(&terminal);
        self.inbox.remove(&terminal).unwrap_or_default()
    }

    #[must_use]
    pub fn unread(&self, terminal: u64) -> usize {
        self.inbox.get(&terminal).map_or(0, Vec::len)
    }

    pub fn notify(&mut self, terminal: u64) -> Option<usize> {
        let unread = self.unread(terminal);
        (unread > 0 && self.notified.insert(terminal)).then_some(unread)
    }

    pub fn forget(&mut self, terminal: u64) {
        self.inbox.remove(&terminal);
        self.notified.remove(&terminal);
    }
}

#[must_use]
pub fn notice_text(cli: &str, count: usize) -> String {
    format!("You have {count} new message(s) from other agents. Run `{cli} check` to read them.")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SENDER: u64 = 1;
    const RECIPIENT: u64 = 2;

    fn post(mailbox: &mut Mailbox, body: &str) -> u64 {
        mailbox.post(SENDER, "Codex · 1".to_owned(), RECIPIENT, body.to_owned())
    }

    #[test]
    fn take_drains_messages_in_order_with_increasing_ids() {
        let mut mailbox = Mailbox::default();
        assert_eq!(post(&mut mailbox, "a"), 1);
        assert_eq!(post(&mut mailbox, "b"), 2);
        assert_eq!(mailbox.unread(RECIPIENT), 2);
        let bodies: Vec<String> = mailbox
            .take(RECIPIENT)
            .into_iter()
            .map(|message| message.body)
            .collect();
        assert_eq!(bodies, ["a", "b"]);
        assert_eq!(mailbox.unread(RECIPIENT), 0);
    }

    #[test]
    fn notifies_once_until_new_mail_arrives() {
        let mut mailbox = Mailbox::default();
        post(&mut mailbox, "a");
        assert_eq!(mailbox.notify(RECIPIENT), Some(1));
        assert_eq!(mailbox.notify(RECIPIENT), None);
        post(&mut mailbox, "b");
        assert_eq!(mailbox.notify(RECIPIENT), Some(2));
        mailbox.forget(RECIPIENT);
        assert_eq!(mailbox.notify(RECIPIENT), None);
    }
}
