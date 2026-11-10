use crate::agent::AgentPreset;
use crate::environment::EnvironmentData;
use crate::ids::{GroupId, ResumeId, SessionId, TerminalId};
use crate::model::{lenient_model, ModelId};
use crate::usage::Spend;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionLead {
    pub preset: AgentPreset,
    #[serde(default, deserialize_with = "lenient_model")]
    pub model: Option<ModelId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workers: Vec<Worker>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal: Option<TerminalId>,
}

impl SessionLead {
    #[must_use]
    pub const fn new(preset: AgentPreset, model: Option<ModelId>) -> Self {
        Self {
            preset,
            model,
            workers: Vec::new(),
            terminal: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Worker {
    pub terminal: TerminalId,
    pub task: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<SessionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reopen: Option<WorkerReopen>,
    #[serde(default, skip_serializing_if = "Spend::is_empty")]
    pub spent: Spend,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerReopen {
    pub harness: AgentPreset,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<GroupId>,
    pub resume: ResumeId,
}

impl EnvironmentData {
    #[must_use]
    pub fn lead(&self, id: SessionId) -> Option<&SessionLead> {
        self.session(id)?.lead.as_ref()
    }

    pub(super) fn lead_mut(&mut self, id: SessionId) -> Option<&mut SessionLead> {
        self.sessions
            .iter_mut()
            .find(|session| session.id == id)?
            .lead
            .as_mut()
    }

    pub fn dismiss_lead(&mut self, id: SessionId) -> Option<SessionLead> {
        self.sessions
            .iter_mut()
            .find(|session| session.id == id)
            .and_then(|session| session.lead.take())
    }

    #[must_use]
    pub fn session_led_by(&self, terminal: TerminalId) -> Option<SessionId> {
        self.sessions
            .iter()
            .find(|session| {
                session
                    .lead
                    .as_ref()
                    .is_some_and(|lead| lead.terminal == Some(terminal))
            })
            .map(|session| session.id)
    }

    pub(crate) fn drop_worker(&mut self, terminal: TerminalId) {
        for session in &mut self.sessions {
            if session
                .lead
                .as_ref()
                .is_some_and(|lead| lead.terminal == Some(terminal))
            {
                session.lead = None;
            }
            let workers = session.lead.iter_mut().flat_map(|lead| &mut lead.workers);
            for worker in workers.filter(|worker| worker.terminal == terminal) {
                worker.session = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::seed_environment;
    use crate::group::WorkerSpawn;

    fn claude() -> SessionLead {
        SessionLead::new(AgentPreset::ClaudeCode, None)
    }

    fn led_session() -> (EnvironmentData, SessionId) {
        let mut environment = seed_environment();
        environment.enter_sessions();
        let id = environment
            .active_session
            .unwrap_or_else(|| unreachable!("Sessions mode opens a session"));
        if let Some(session) = environment
            .sessions
            .iter_mut()
            .find(|session| session.id == id)
        {
            session.lead = Some(claude());
        }
        (environment, id)
    }

    #[test]
    fn closing_a_workers_tab_keeps_it_as_closed() {
        let (mut environment, id) = led_session();
        let terminal = environment.nodes[0].id;
        let spawn = WorkerSpawn {
            lead: Some(id),
            task: "webhook".to_owned(),
        };
        environment.open_worker_session(terminal, spawn);
        environment.remove_terminal_tiles(terminal);
        let workers = environment.lead(id).map(|lead| lead.workers.clone());
        assert_eq!(
            workers,
            Some(vec![Worker {
                terminal,
                task: "webhook".to_owned(),
                session: None,
                reopen: None,
                spent: Spend::default(),
            }])
        );
    }

    #[test]
    fn dismissing_a_lead_hands_back_its_workers() {
        let (mut environment, id) = led_session();
        let dismissed = environment.dismiss_lead(id);
        assert_eq!(dismissed, Some(claude()));
        assert_eq!(environment.lead(id), None);
    }
}
