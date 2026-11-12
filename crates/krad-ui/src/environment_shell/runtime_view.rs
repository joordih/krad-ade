use futures::StreamExt as _;
use gpui_kit::Context;
use krad_domain::{AgentStatus, EnvironmentName, TerminalId};
use krad_protocol::{AgentRef, AskId, BusSnapshot, EventKind, Feed, MailCount, StatusReport};
use krad_runtime::RuntimeHandle;
use std::collections::{BTreeSet, HashMap};

use super::EnvironmentShell;

type AgentSpot = (EnvironmentName, TerminalId);

fn spot(agent: AgentRef) -> AgentSpot {
    (agent.environment, agent.terminal)
}

pub(super) struct RuntimeLink {
    pub(super) handle: RuntimeHandle,
    pub(super) view: RuntimeView,
}

impl RuntimeLink {
    pub(super) fn new(handle: RuntimeHandle) -> Self {
        Self {
            handle,
            view: RuntimeView::default(),
        }
    }
}

#[derive(Default)]
pub(super) struct RuntimeView {
    statuses: HashMap<AgentSpot, AgentStatus>,
    asks: BTreeSet<AskId>,
    pub(super) mail: MailNotices,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Unread {
    count: usize,
    noticed: bool,
}

#[derive(Default)]
pub(super) struct MailNotices(HashMap<AgentSpot, Unread>);

impl MailNotices {
    fn set(&mut self, mail: MailCount) {
        let key = spot(mail.agent);
        if mail.unread == 0 {
            self.0.remove(&key);
            return;
        }
        let unread = self.0.entry(key).or_insert(Unread {
            count: 0,
            noticed: false,
        });
        if mail.unread > unread.count {
            unread.noticed = false;
        }
        unread.count = mail.unread;
    }

    fn replace(&mut self, counts: Vec<MailCount>) {
        let known = std::mem::take(&mut self.0);
        for mail in counts {
            let key = spot(mail.agent.clone());
            let noticed = known
                .get(&key)
                .is_some_and(|unread| unread.noticed && unread.count >= mail.unread);
            self.0.insert(
                key,
                Unread {
                    count: mail.unread,
                    noticed,
                },
            );
        }
    }

    pub(super) fn notify(&mut self, env: &EnvironmentName, id: TerminalId) -> Option<usize> {
        let unread = self.0.get_mut(&(env.clone(), id))?;
        if unread.noticed {
            return None;
        }
        unread.noticed = true;
        Some(unread.count)
    }
}

impl EnvironmentShell {
    pub(super) fn follow_runtime(&mut self, cx: &mut Context<Self>) {
        let mut feed = self.runtime.handle.subscribe();
        cx.spawn(async move |this, cx| {
            while let Some(feed) = feed.next().await {
                if this
                    .update(cx, |shell, cx| shell.apply_runtime_feed(feed, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    pub(super) fn agent_status(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
    ) -> Option<&AgentStatus> {
        self.runtime.view.statuses.get(&(env.clone(), id))
    }

    fn apply_runtime_feed(&mut self, feed: Feed, cx: &mut Context<Self>) {
        match feed {
            Feed::Snapshot(snapshot) => self.apply_bus_snapshot(snapshot.bus),
            Feed::Event(event) => self.apply_runtime_event(event.kind),
        }
        cx.notify();
    }

    fn apply_runtime_event(&mut self, kind: EventKind) {
        match kind {
            EventKind::AgentStatus(report) => self.agent_status_heard(report),
            EventKind::Mail(mail) => self.runtime.view.mail.set(mail),
            EventKind::AskPending(pending) => {
                if self.runtime.view.asks.insert(pending.ask) {
                    self.hold_ask(pending);
                }
            }
            EventKind::AskSettled { ask, .. } => {
                if self.runtime.view.asks.remove(&ask) {
                    self.forget_held_ask(ask);
                }
            }
            EventKind::Pong { .. } | EventKind::Unknown => {}
        }
    }

    fn agent_status_heard(&mut self, report: StatusReport) {
        let host = self.env_link(&report.agent.environment).host();
        self.statusline_heard(host, &report.status);
        self.runtime
            .view
            .statuses
            .insert(spot(report.agent), report.status);
    }

    fn apply_bus_snapshot(&mut self, bus: BusSnapshot) {
        self.runtime.view.statuses = bus
            .statuses
            .into_iter()
            .map(|report| (spot(report.agent), report.status))
            .collect();
        self.runtime.view.mail.replace(bus.mail);
        let pending: BTreeSet<AskId> = bus.asks.iter().map(|pending| pending.ask).collect();
        let settled: Vec<AskId> = self
            .runtime
            .view
            .asks
            .difference(&pending)
            .copied()
            .collect();
        for ask in settled {
            self.runtime.view.asks.remove(&ask);
            self.forget_held_ask(ask);
        }
        for pending in bus.asks {
            if self.runtime.view.asks.insert(pending.ask) {
                self.hold_ask(pending);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mail(unread: usize) -> MailCount {
        MailCount {
            agent: AgentRef {
                environment: EnvironmentName::new("main".to_owned()),
                terminal: TerminalId(2),
            },
            unread,
        }
    }

    #[test]
    fn an_agent_is_told_of_its_mail_once_until_more_arrives() {
        let env = EnvironmentName::new("main".to_owned());
        let mut notices = MailNotices::default();
        assert_eq!(notices.notify(&env, TerminalId(2)), None);
        notices.set(mail(1));
        assert_eq!(notices.notify(&env, TerminalId(2)), Some(1));
        assert_eq!(notices.notify(&env, TerminalId(2)), None);
        notices.set(mail(2));
        assert_eq!(notices.notify(&env, TerminalId(2)), Some(2));
        notices.set(mail(0));
        assert_eq!(notices.notify(&env, TerminalId(2)), None);
    }

    #[test]
    fn a_snapshot_keeps_a_notice_already_given_for_the_same_mail() {
        let env = EnvironmentName::new("main".to_owned());
        let mut notices = MailNotices::default();
        notices.set(mail(1));
        assert_eq!(notices.notify(&env, TerminalId(2)), Some(1));
        notices.replace(vec![mail(1)]);
        assert_eq!(notices.notify(&env, TerminalId(2)), None);
        notices.replace(vec![mail(3)]);
        assert_eq!(notices.notify(&env, TerminalId(2)), Some(3));
    }
}
