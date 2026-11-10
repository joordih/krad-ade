mod code_host;

use gpui_kit::Task;
use krad_domain::{
    AgentPreset, AgentState, AppEvent, EnvironmentName, EventRecord, GroupId, Host, Session,
    SessionId, TerminalId,
};
use krad_store::{EventJournal, JournalSend};
use std::collections::HashMap;
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
}

impl AppEvents {
    pub(super) fn start() -> Self {
        Self {
            journal: EventJournal::spawn(),
            webhooks: WebhookSender::spawn(),
            webhook_secrets: None,
            sessions: HashMap::new(),
            code_hosts: code_host::CodeHostWatch::default(),
        }
    }
}

pub(super) fn agent_event(
    terminal: TerminalId,
    harness: Option<AgentPreset>,
    before: AgentState,
    after: AgentState,
) -> Option<AppEvent> {
    if !harness.is_some_and(|harness| harness != AgentPreset::Shell) {
        return None;
    }
    match (before, after) {
        (AgentState::Working, AgentState::Working) => None,
        (_, AgentState::Working) => Some(AppEvent::AgentStarted { terminal, harness }),
        (_, AgentState::WaitingForInput) => Some(AppEvent::AgentWaiting { terminal }),
        (_, AgentState::Done) | (AgentState::Working, AgentState::Idle) => {
            Some(AppEvent::AgentFinished { terminal })
        }
        _ => None,
    }
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
            for hook in self.settings.webhooks.wanting(kind) {
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

    const TERMINAL: TerminalId = TerminalId(5);

    #[test]
    fn agent_states_become_started_waiting_and_finished() {
        let claude = Some(AgentPreset::ClaudeCode);
        let event = |before, after| agent_event(TERMINAL, claude, before, after);
        assert_eq!(
            event(AgentState::Idle, AgentState::Working),
            Some(AppEvent::AgentStarted {
                terminal: TERMINAL,
                harness: claude,
            })
        );
        assert_eq!(
            event(AgentState::Working, AgentState::WaitingForInput),
            Some(AppEvent::AgentWaiting { terminal: TERMINAL })
        );
        let finished = Some(AppEvent::AgentFinished { terminal: TERMINAL });
        assert_eq!(event(AgentState::Working, AgentState::Done), finished);
        assert_eq!(event(AgentState::Working, AgentState::Idle), finished);
        assert_eq!(event(AgentState::Done, AgentState::Idle), None);
        assert_eq!(event(AgentState::Working, AgentState::Working), None);
    }

    #[test]
    fn plain_shells_raise_no_agent_events() {
        let shell = |harness| agent_event(TERMINAL, harness, AgentState::Idle, AgentState::Working);
        assert_eq!(shell(None), None);
        assert_eq!(shell(Some(AgentPreset::Shell)), None);
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
