mod stored;

use crate::acp::{AcpAgent, AcpAgentId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum AgentState {
    Idle,
    Working,
    WaitingForInput,
    Done,
}

impl AgentState {
    pub const ALL: [Self; 4] = [Self::Idle, Self::Working, Self::WaitingForInput, Self::Done];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            AgentState::Idle => "idle",
            AgentState::Working => "working",
            AgentState::WaitingForInput => "waiting-for-input",
            AgentState::Done => "done",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnknownAgentState;

impl std::fmt::Display for UnknownAgentState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("unknown agent state")
    }
}

impl std::error::Error for UnknownAgentState {}

impl std::str::FromStr for AgentState {
    type Err = UnknownAgentState;

    fn from_str(label: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|state| state.label() == label)
            .ok_or(UnknownAgentState)
    }
}

pub const AGENT_WAIT_SECS: f32 = 3.0;

const CLAUDE_SYSTEM_PROMPT_FLAG: &str = "--append-system-prompt";
const CLAUDE_MODEL_FLAG: &str = "--model";
const CODEX_MODEL_FLAG: &str = "-m";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelDelivery {
    Flag(&'static str),
    AttachedSession,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum AgentPreset {
    #[default]
    Shell,
    ClaudeCode,
    Codex,
    Opencode,
    Gateway,
    Acp(AcpAgentId),
}

pub const ACP_AGENT_LABEL: &str = "ACP agent";
const GATEWAY_NOT_IN_CLOUD: &str =
    "Gateway chats run inside the app, so cloud sessions cannot keep them going";

impl AgentPreset {
    pub const NATIVE: [Self; 5] = [
        Self::Shell,
        Self::ClaudeCode,
        Self::Codex,
        Self::Opencode,
        Self::Gateway,
    ];

    pub fn listed(acp_agents: &[AcpAgent]) -> impl Iterator<Item = Self> + '_ {
        Self::NATIVE
            .into_iter()
            .chain(acp_agents.iter().map(|agent| Self::Acp(agent.id)))
    }

    #[must_use]
    pub const fn acp_agent(self) -> Option<AcpAgentId> {
        match self {
            Self::Acp(id) => Some(id),
            Self::Shell | Self::ClaudeCode | Self::Codex | Self::Opencode | Self::Gateway => None,
        }
    }

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Shell => "Shell",
            Self::ClaudeCode => "Claude Code",
            Self::Codex => "Codex",
            Self::Opencode => "opencode",
            Self::Gateway => "Gateway",
            Self::Acp(_) => ACP_AGENT_LABEL,
        }
    }

    #[must_use]
    pub const fn program(self) -> Option<&'static str> {
        match self {
            Self::Shell | Self::Gateway | Self::Acp(_) => None,
            Self::ClaudeCode => Some("claude"),
            Self::Codex => Some("codex"),
            Self::Opencode => Some("opencode"),
        }
    }

    #[must_use]
    pub const fn system_prompt_flag(self) -> Option<&'static str> {
        match self {
            Self::ClaudeCode => Some(CLAUDE_SYSTEM_PROMPT_FLAG),
            Self::Shell | Self::Codex | Self::Opencode | Self::Gateway | Self::Acp(_) => None,
        }
    }

    #[must_use]
    pub const fn model_delivery(self) -> Option<ModelDelivery> {
        match self {
            Self::ClaudeCode => Some(ModelDelivery::Flag(CLAUDE_MODEL_FLAG)),
            Self::Codex => Some(ModelDelivery::Flag(CODEX_MODEL_FLAG)),
            Self::Opencode => Some(ModelDelivery::AttachedSession),
            Self::Shell | Self::Gateway | Self::Acp(_) => None,
        }
    }

    #[must_use]
    pub const fn unavailable_in_cloud(self) -> Option<&'static str> {
        match self {
            Self::Gateway => Some(GATEWAY_NOT_IN_CLOUD),
            Self::Shell | Self::ClaudeCode | Self::Codex | Self::Opencode | Self::Acp(_) => None,
        }
    }

    #[must_use]
    pub const fn is_agent(self) -> bool {
        match self {
            Self::ClaudeCode | Self::Codex | Self::Opencode | Self::Acp(_) => true,
            Self::Shell | Self::Gateway => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnknownHarness;

impl std::fmt::Display for UnknownHarness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("unknown harness program")
    }
}

impl std::error::Error for UnknownHarness {}

impl std::str::FromStr for AgentPreset {
    type Err = UnknownHarness;

    fn from_str(program: &str) -> Result<Self, Self::Err> {
        Self::NATIVE
            .into_iter()
            .find(|preset| preset.program() == Some(program))
            .ok_or(UnknownHarness)
    }
}

#[must_use]
pub fn classify_agent_state(
    idle_secs: f32,
    child_alive: bool,
    bell: bool,
    is_agent: bool,
) -> AgentState {
    if !child_alive || !is_agent {
        return AgentState::Idle;
    }
    if bell || idle_secs >= AGENT_WAIT_SECS {
        return AgentState::WaitingForInput;
    }
    AgentState::Working
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_agent_has_its_own_program() {
        let programs: Vec<&str> = AgentPreset::NATIVE
            .iter()
            .filter_map(|preset| preset.program())
            .collect();
        let unique: std::collections::HashSet<&&str> = programs.iter().collect();
        assert_eq!(unique.len(), programs.len());
    }

    #[test]
    fn harnesses_parse_from_their_program_and_states_from_their_label() {
        for preset in AgentPreset::NATIVE {
            let parsed = preset.program().map(str::parse::<AgentPreset>);
            assert_eq!(parsed, preset.program().map(|_| Ok(preset)), "{preset:?}");
        }
        assert_eq!("Claude Code".parse::<AgentPreset>(), Err(UnknownHarness));
        for state in AgentState::ALL {
            assert_eq!(state.label().parse(), Ok(state));
        }
        assert_eq!("exited".parse::<AgentState>(), Err(UnknownAgentState));
    }

    #[test]
    fn only_gateway_chats_cannot_run_in_a_cloud_session() {
        let unavailable: Vec<AgentPreset> = AgentPreset::NATIVE
            .into_iter()
            .filter(|preset| preset.unavailable_in_cloud().is_some())
            .collect();
        assert_eq!(unavailable, [AgentPreset::Gateway]);
    }

    #[test]
    fn agents_work_while_printing_and_wait_once_silent() {
        assert_eq!(
            classify_agent_state(0.5, true, false, true),
            AgentState::Working
        );
        assert_eq!(
            classify_agent_state(0.5, true, true, true),
            AgentState::WaitingForInput,
            "a bell asks for the user at once"
        );
        assert_eq!(
            classify_agent_state(AGENT_WAIT_SECS, true, false, true),
            AgentState::WaitingForInput,
            "a TUI sitting silent at its prompt"
        );
        assert_eq!(
            classify_agent_state(0.1, true, false, false),
            AgentState::Idle,
            "a shell printing its prompt is not working"
        );
        assert_eq!(
            classify_agent_state(0.1, false, false, true),
            AgentState::Idle
        );
    }
}
