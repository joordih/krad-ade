use gpui_kit::Task;
use krad_domain::{AppEvent, EnvironmentName, EventRecord, GroupId, Session, SessionId, WebhookId};
use krad_protocol::{Command, EventsCommand};
use std::collections::{BTreeSet, HashMap};

use super::EnvironmentShell;

type SessionKey = (SessionId, Option<GroupId>);

#[derive(Default)]
pub(super) struct AppEventsUi {
    sessions: HashMap<EnvironmentName, Vec<SessionKey>>,
    delivered: BTreeSet<WebhookId>,
    pub(super) webhook_secrets: Option<Task<()>>,
}

fn same_sessions(seen: Option<&Vec<SessionKey>>, sessions: &[Session]) -> bool {
    seen.is_some_and(|seen| {
        seen.len() == sessions.len()
            && seen
                .iter()
                .zip(sessions)
                .all(|(key, session)| *key == (session.id, session.group))
    })
}

fn session_events(before: &[SessionKey], now: &[SessionKey]) -> Vec<(Option<GroupId>, AppEvent)> {
    let opened = now
        .iter()
        .filter(|key| !before.contains(key))
        .map(|&(session, group)| (group, AppEvent::SessionOpened { session }));
    let closed = before
        .iter()
        .filter(|key| !now.contains(key))
        .map(|&(session, group)| (group, AppEvent::SessionClosed { session }));
    opened.chain(closed).collect()
}

impl EnvironmentShell {
    pub(super) fn publish_event(
        &mut self,
        environment: &EnvironmentName,
        group: Option<GroupId>,
        event: AppEvent,
    ) {
        self.runtime
            .handle
            .post(Command::Events(EventsCommand::Publish {
                environment: environment.clone(),
                group,
                event,
                delivered: self.app_events.delivered.clone(),
            }));
    }

    pub(super) fn use_webhooks(&self) {
        self.runtime
            .handle
            .post(Command::Events(EventsCommand::UseWebhooks {
                webhooks: self.settings.webhooks.clone(),
            }));
    }

    pub(super) fn app_event_heard(&mut self, record: &EventRecord) {
        self.announce_app_event(record);
        self.variant_event_heard(record);
    }

    pub(super) fn with_webhooks_delivered(
        &mut self,
        delivered: BTreeSet<WebhookId>,
        publish: impl FnOnce(&mut Self),
    ) {
        self.app_events.delivered = delivered;
        publish(self);
        self.app_events.delivered.clear();
    }

    pub(super) fn sync_app_events(&mut self) {
        let changed: Vec<(EnvironmentName, Vec<SessionKey>)> = self
            .envs
            .iter()
            .filter(|(name, live)| {
                !same_sessions(self.app_events.sessions.get(*name), &live.data.sessions)
            })
            .map(|(name, live)| {
                let keys = live
                    .data
                    .sessions
                    .iter()
                    .map(|session| (session.id, session.group));
                (name.clone(), keys.collect())
            })
            .collect();
        for (name, now) in changed {
            let Some(before) = self.app_events.sessions.insert(name.clone(), now.clone()) else {
                continue;
            };
            for (group, event) in session_events(&before, &now) {
                self.publish_event(&name, group, event);
            }
        }
        if self.app_events.sessions.len() > self.envs.len() {
            let open = &self.envs;
            self.app_events
                .sessions
                .retain(|name, _| open.contains_key(name));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sessions_that_appear_open_and_those_that_vanish_close() {
        let before = [(SessionId(1), Some(GroupId(1))), (SessionId(2), None)];
        let now = [(SessionId(2), None), (SessionId(3), Some(GroupId(1)))];
        assert_eq!(
            session_events(&before, &now),
            vec![
                (
                    Some(GroupId(1)),
                    AppEvent::SessionOpened {
                        session: SessionId(3)
                    }
                ),
                (
                    Some(GroupId(1)),
                    AppEvent::SessionClosed {
                        session: SessionId(1)
                    }
                ),
            ]
        );
        assert_eq!(session_events(&now, &now), Vec::new());
    }
}
