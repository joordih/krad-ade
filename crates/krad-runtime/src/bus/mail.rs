use futures::channel::oneshot;
use krad_agents::{AgentKey, Mailbox, Response, TerminalHandle};
use krad_protocol::{EventKind, MailCount};
use std::collections::HashMap;

use super::delegation::Mail;
use super::{agent_of, reply_with};

#[derive(Debug, Default)]
pub(crate) struct Mailboxes {
    boxes: HashMap<String, Mailbox>,
    waiters: Vec<(TerminalHandle, oneshot::Sender<Response>)>,
}

impl Mailboxes {
    pub(crate) fn check(
        &mut self,
        from: TerminalHandle,
        wait_secs: Option<u64>,
        reply: oneshot::Sender<Response>,
        outbox: &mut Vec<EventKind>,
    ) {
        let messages = self.take(&from, outbox);
        if messages.is_empty() && wait_secs.is_some() {
            self.waiters.push((from, reply));
            return;
        }
        reply_with(reply, Response::Messages(messages));
    }

    pub(crate) fn post(&mut self, mail: Mail, outbox: &mut Vec<EventKind>) -> u64 {
        let Mail {
            from,
            from_title,
            to,
            body,
        } = mail;
        let mailbox = self.boxes.entry(from.environment.clone()).or_default();
        let id = mailbox.post(from.terminal, from_title, to, body);
        let recipient = TerminalHandle {
            environment: from.environment,
            terminal: to,
        };
        if !self.wake(&recipient, outbox) {
            self.count(&recipient, outbox);
        }
        id
    }

    pub(crate) fn forget(&mut self, agent: &AgentKey, outbox: &mut Vec<EventKind>) {
        let Some(mailbox) = self.boxes.get_mut(&agent.environment) else {
            return;
        };
        let had_mail = mailbox.unread(agent.terminal) > 0;
        mailbox.forget(agent.terminal);
        if had_mail {
            outbox.push(EventKind::Mail(MailCount {
                agent: agent_of(&TerminalHandle {
                    environment: agent.environment.clone(),
                    terminal: agent.terminal,
                }),
                unread: 0,
            }));
        }
    }

    pub(crate) fn tick(&mut self) {
        self.waiters.retain(|(_, reply)| !reply.is_canceled());
    }

    pub(crate) fn counts(&self) -> Vec<MailCount> {
        self.boxes
            .iter()
            .flat_map(|(environment, mailbox)| {
                mailbox.unread_counts().map(|(terminal, unread)| MailCount {
                    agent: agent_of(&TerminalHandle {
                        environment: environment.clone(),
                        terminal,
                    }),
                    unread,
                })
            })
            .collect()
    }

    fn take(
        &mut self,
        terminal: &TerminalHandle,
        outbox: &mut Vec<EventKind>,
    ) -> Vec<krad_agents::Message> {
        let Some(mailbox) = self.boxes.get_mut(&terminal.environment) else {
            return Vec::new();
        };
        let messages = mailbox.take(terminal.terminal);
        if !messages.is_empty() {
            self.count(terminal, outbox);
        }
        messages
    }

    fn wake(&mut self, recipient: &TerminalHandle, outbox: &mut Vec<EventKind>) -> bool {
        self.waiters.retain(|(_, reply)| !reply.is_canceled());
        let Some(index) = self
            .waiters
            .iter()
            .position(|(waiter, _)| waiter == recipient)
        else {
            return false;
        };
        let (_, reply) = self.waiters.swap_remove(index);
        let messages = self.take(recipient, outbox);
        reply_with(reply, Response::Messages(messages));
        true
    }

    fn count(&self, agent: &TerminalHandle, outbox: &mut Vec<EventKind>) {
        let unread = self
            .boxes
            .get(&agent.environment)
            .map_or(0, |mailbox| mailbox.unread(agent.terminal));
        outbox.push(EventKind::Mail(MailCount {
            agent: agent_of(agent),
            unread,
        }));
    }
}
