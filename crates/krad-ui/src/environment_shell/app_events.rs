mod code_host;

use gpui_kit::Task;
use krad_domain::{
    AppEvent, EnvironmentName, EventRecord, GroupId, Host, Session, SessionId, Webhook, WebhookId,
};
use krad_store::{EventJournal, JournalSend};
use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::time::SystemTime;

use super::groups::{GitKey, GroupGit};
use super::EnvironmentShell;
use crate::webhook::{Delivery, WebhookSender};

type SessionKey = (SessionId, Option<GroupId>);

pub(super) struct AppEvents {
    journal: Option<EventJournal>,
    pub(super) webhooks: Option<WebhookSender>,
    pub(super) webhook_secrets: Option<Task<()>>,
    sessions: HashMap<EnvironmentName, Vec<SessionKey>>,
    code_hosts: code_host::CodeHostWatch,
    delivered: BTreeSet<WebhookId>,
}

impl AppEvents {
    pub(super) fn start() -> Self {
        Self {
            journal: EventJournal::spawn(),
            webhooks: WebhookSender::spawn(),
            webhook_secrets: None,
            sessions: HashMap::new(),
            code_hosts: code_host::CodeHostWatch::default(),
            delivered: BTreeSet::new(),
        }
    }
}

fn undelivered<'a>(
    wanting: impl Iterator<Item = &'a Webhook>,
    delivered: &'a BTreeSet<WebhookId>,
) -> impl Iterator<Item = &'a Webhook> {
    wanting.filter(move |hook| !delivered.contains(&hook.id))
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

pub(super) fn changed_git_roots(
    before: &HashMap<GitKey, GroupGit>,
    after: &HashMap<GitKey, GroupGit>,
) -> Vec<GitKey> {
    after
        .iter()
        .filter(|(key, now)| before.get(*key).is_some_and(|was| was.status != now.status))
        .map(|(key, _)| key.clone())
        .collect()
}

impl EnvironmentShell {
    pub(super) fn publish_event(
        &mut self,
        environment: &EnvironmentName,
        group: Option<GroupId>,
        event: AppEvent,
    ) {
        let record = EventRecord::new(SystemTime::now(), environment.clone(), group, event);
        let kind = record.event.kind();
        tracing::trace!(?kind, "app event");
        self.observe_kraddie_event(environment, group, &record.event);
        self.announce_app_event(&record);
        if let Some(sender) = &self.app_events.webhooks {
            let wanting = self.settings.webhooks.wanting(kind);
            for hook in undelivered(wanting, &self.app_events.delivered) {
                sender.send(Delivery {
                    hook: hook.clone(),
                    record: record.clone(),
                });
            }
        }
        let Some(journal) = &self.app_events.journal else {
            return;
        };
        match journal.record(record) {
            JournalSend::Queued => {}
            JournalSend::Full => tracing::warn!(?kind, "event journal full; event not kept"),
            JournalSend::Stopped => tracing::debug!("event journal stopped"),
        }
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

    pub(super) fn announce_git_changes(&mut self, roots: &[GitKey]) {
        let changed: Vec<(EnvironmentName, GroupId)> = self
            .envs
            .iter()
            .flat_map(|(name, live)| {
                live.data
                    .groups
                    .iter()
                    .filter(|group| is_changed(roots, &live.data.host, &group.root))
                    .map(|group| (name.clone(), group.id))
            })
            .collect();
        for (name, group) in changed {
            self.publish_event(&name, Some(group), AppEvent::GitChanged);
        }
    }
}

fn is_changed(roots: &[GitKey], host: &Host, root: &Path) -> bool {
    roots
        .iter()
        .any(|(changed_host, changed_root)| changed_host == host && changed_root == root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webhooks_the_runner_delivered_are_left_out() -> Result<(), Box<dyn std::error::Error>> {
        let hook = |id: u64| -> Result<Webhook, Box<dyn std::error::Error>> {
            Ok(Webhook {
                id: WebhookId(id),
                name: "ci".parse()?,
                url: "https://hooks.example.test/krad".parse()?,
                kinds: BTreeSet::new(),
                signing: krad_domain::WebhookSigning::Unsigned,
            })
        };
        let hooks = [hook(1)?, hook(2)?];
        let left = |delivered: &BTreeSet<WebhookId>| -> Vec<WebhookId> {
            undelivered(hooks.iter(), delivered)
                .map(|hook| hook.id)
                .collect()
        };
        assert_eq!(left(&BTreeSet::new()), vec![WebhookId(1), WebhookId(2)]);
        assert_eq!(
            left(&[WebhookId(2)].into_iter().collect()),
            vec![WebhookId(1)]
        );
        Ok(())
    }

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
