use crate::protocol::{AgentInfo, TerminalState};
use krad_domain::AgentState;
use serde::Serialize;

const NONE: &str = "none";

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub(super) struct AgentStates {
    pub(super) working: Vec<AgentInfo>,
    pub(super) waiting_for_input: Vec<AgentInfo>,
    pub(super) done: Vec<AgentInfo>,
    pub(super) idle: Vec<AgentInfo>,
    pub(super) exited: Vec<AgentInfo>,
}

impl AgentStates {
    pub(super) fn sort(agents: &[AgentInfo]) -> Self {
        let mut states = Self::default();
        for agent in agents.iter().filter(|agent| agent.harness.is_some()) {
            let bucket = match agent.state {
                TerminalState::Running(AgentState::Working) => &mut states.working,
                TerminalState::Running(AgentState::WaitingForInput) => {
                    &mut states.waiting_for_input
                }
                TerminalState::Running(AgentState::Done) => &mut states.done,
                TerminalState::Running(AgentState::Idle) => &mut states.idle,
                TerminalState::Exited | TerminalState::Closed => &mut states.exited,
            };
            bucket.push(agent.clone());
        }
        states
    }

    pub(super) fn render(&self) -> String {
        [
            (TerminalState::Running(AgentState::Working), &self.working),
            (
                TerminalState::Running(AgentState::WaitingForInput),
                &self.waiting_for_input,
            ),
            (TerminalState::Running(AgentState::Done), &self.done),
            (TerminalState::Running(AgentState::Idle), &self.idle),
            (TerminalState::Exited, &self.exited),
        ]
        .into_iter()
        .map(|(state, agents)| format!("{state} ({}): {}", agents.len(), names(agents)))
        .collect::<Vec<_>>()
        .join("\n")
    }
}

fn names(agents: &[AgentInfo]) -> String {
    if agents.is_empty() {
        return NONE.to_owned();
    }
    agents
        .iter()
        .map(|agent| format!("{} {}", agent.terminal, agent.title))
        .collect::<Vec<_>>()
        .join(", ")
}
