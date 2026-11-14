use krad_domain::{AppEvent, EnvironmentData, EnvironmentName, GroupId, SessionId};
use krad_protocol::EventKind;
use std::collections::HashMap;

use super::Runtime;

type SessionKey = (SessionId, Option<GroupId>);

#[derive(Debug)]
pub(crate) struct SessionsSeen(HashMap<EnvironmentName, Vec<SessionKey>>);

fn keys(data: &EnvironmentData) -> Vec<SessionKey> {
    data.sessions
        .iter()
        .map(|session| (session.id, session.group))
        .collect()
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

const fn moves_environments(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::EnvironmentChanged { .. }
            | EventKind::EnvironmentOpened { .. }
            | EventKind::EnvironmentClosed { .. }
    )
}

impl SessionsSeen {
    pub(super) fn new<'a>(environments: impl Iterator<Item = &'a EnvironmentData>) -> Self {
        Self(
            environments
                .map(|data| (data.name.clone(), keys(data)))
                .collect(),
        )
    }

    fn moved<'a>(
        &mut self,
        environments: impl Iterator<Item = &'a EnvironmentData>,
    ) -> Vec<(EnvironmentName, Option<GroupId>, AppEvent)> {
        let mut events = Vec::new();
        let mut open = Vec::new();
        for data in environments {
            open.push(data.name.clone());
            let now = keys(data);
            let Some(before) = self.0.insert(data.name.clone(), now.clone()) else {
                continue;
            };
            events.extend(
                session_events(&before, &now)
                    .into_iter()
                    .map(|(group, event)| (data.name.clone(), group, event)),
            );
        }
        self.0.retain(|name, _| open.contains(name));
        events
    }
}

impl Runtime {
    pub(super) fn publish_session_events(&mut self) {
        if !self.outbox.iter().any(moves_environments) {
            return;
        }
        for (environment, group, event) in self.sessions_seen.moved(self.environments.iter()) {
            self.events
                .publish_own(&environment, group, &event, &mut self.outbox);
        }
    }
}

#[cfg(test)]
mod tests;
