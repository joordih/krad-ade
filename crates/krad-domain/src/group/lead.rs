use crate::agent::AgentPreset;
use crate::environment::EnvironmentData;
use crate::ids::{SessionId, TerminalId};
use crate::model::ModelId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionLead {
    pub preset: AgentPreset,
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
}

impl EnvironmentData {
    #[must_use]
    pub fn lead(&self, id: SessionId) -> Option<&SessionLead> {
        self.session(id)?.lead.as_ref()
    }

    fn lead_mut(&mut self, id: SessionId) -> Option<&mut SessionLead> {
        self.sessions
            .iter_mut()
            .find(|session| session.id == id)?
            .lead
            .as_mut()
    }

    pub fn dismiss_lead(&mut self, id: SessionId) -> bool {
        self.sessions
            .iter_mut()
            .find(|session| session.id == id)
            .and_then(|session| session.lead.take())
            .is_some()
    }

    pub fn add_worker(&mut self, id: SessionId, terminal: TerminalId, task: String) -> bool {
        let Some(lead) = self.lead_mut(id) else {
            return false;
        };
        lead.workers.push(Worker { terminal, task });
        if self.session_of(terminal) != Some(id) {
            for session in &mut self.sessions {
                session.tabs.remove_terminal(terminal);
            }
            if let Some(session) = self.sessions.iter_mut().find(|session| session.id == id) {
                session.tabs.add_terminal(terminal);
            }
        }
        true
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
            if let Some(lead) = session.lead.as_mut() {
                lead.workers.retain(|worker| worker.terminal != terminal);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::seed_environment;

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
    fn closing_a_worker_drops_it() {
        let (mut environment, id) = led_session();
        environment.add_worker(id, TerminalId(1), "webhook".to_owned());
        environment.remove_terminal_tiles(TerminalId(1));
        assert_eq!(environment.lead(id), Some(&claude()));
    }
}
