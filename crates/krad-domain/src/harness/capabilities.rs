use crate::acp::AcpAgent;
use crate::agent::AgentPreset;
use crate::model::Effort;

pub const DEFAULT_CHAT_HARNESS: AgentPreset = AgentPreset::ClaudeCode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalChat {
    InPlace,
    Reopened,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerSurface {
    Terminal,
    Chat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ChatPreference {
    First,
    Second,
    Third,
    Fourth,
    AcpAgent,
}

impl AgentPreset {
    const fn chat_preference(self) -> Option<ChatPreference> {
        match self {
            Self::ClaudeCode => Some(ChatPreference::First),
            Self::Opencode => Some(ChatPreference::Second),
            Self::Codex => Some(ChatPreference::Third),
            Self::Gateway => Some(ChatPreference::Fourth),
            Self::Acp(_) => Some(ChatPreference::AcpAgent),
            Self::Shell => None,
        }
    }

    #[must_use]
    pub const fn has_structured_chat(self) -> bool {
        self.chat_preference().is_some()
    }

    #[must_use]
    pub fn chat_harnesses(acp_agents: &[AcpAgent]) -> Vec<Self> {
        let mut harnesses: Vec<Self> = Self::listed(acp_agents)
            .filter(|preset| preset.has_structured_chat())
            .collect();
        harnesses.sort_by_key(|preset| preset.chat_preference());
        harnesses
    }

    #[must_use]
    pub const fn terminal_chat(self) -> Option<TerminalChat> {
        match self {
            Self::ClaudeCode => Some(TerminalChat::InPlace),
            Self::Codex | Self::Opencode => Some(TerminalChat::Reopened),
            Self::Shell | Self::Gateway | Self::Acp(_) => None,
        }
    }

    #[must_use]
    pub fn terminal_command(self, acp_agents: &[AcpAgent]) -> Option<&str> {
        match self {
            Self::Acp(id) => acp_agents
                .iter()
                .find(|agent| agent.id == id)
                .and_then(|agent| agent.terminal.as_deref()),
            Self::Shell | Self::ClaudeCode | Self::Codex | Self::Opencode | Self::Gateway => {
                self.program()
            }
        }
    }

    #[must_use]
    pub const fn reopens_as_chat(self) -> bool {
        matches!(self.terminal_chat(), Some(TerminalChat::Reopened))
    }

    #[must_use]
    pub const fn reads_shared_skills(self) -> bool {
        match self {
            Self::ClaudeCode => true,
            Self::Shell | Self::Codex | Self::Opencode | Self::Gateway | Self::Acp(_) => false,
        }
    }

    #[must_use]
    pub const fn offered_efforts(self) -> &'static [Effort] {
        match self {
            Self::ClaudeCode => &Effort::ALL,
            Self::Shell | Self::Codex | Self::Opencode | Self::Gateway | Self::Acp(_) => &[],
        }
    }

    #[must_use]
    pub const fn takes_settings_models(self) -> bool {
        match self {
            Self::ClaudeCode => true,
            Self::Shell | Self::Codex | Self::Opencode | Self::Gateway | Self::Acp(_) => false,
        }
    }

    #[must_use]
    pub const fn reports_account_usage(self) -> bool {
        match self {
            Self::ClaudeCode => true,
            Self::Shell | Self::Codex | Self::Opencode | Self::Gateway | Self::Acp(_) => false,
        }
    }

    #[must_use]
    pub const fn chats_through_gateway(self) -> bool {
        match self {
            Self::Gateway => true,
            Self::Shell | Self::ClaudeCode | Self::Codex | Self::Opencode | Self::Acp(_) => false,
        }
    }

    #[must_use]
    pub const fn attaches_to_server(self) -> bool {
        match self {
            Self::Opencode => true,
            Self::Shell | Self::ClaudeCode | Self::Codex | Self::Gateway | Self::Acp(_) => false,
        }
    }

    #[must_use]
    pub const fn worker_surface(self) -> WorkerSurface {
        match self {
            Self::ClaudeCode | Self::Codex | Self::Opencode | Self::Acp(_) => WorkerSurface::Chat,
            Self::Shell | Self::Gateway => WorkerSurface::Terminal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acp::{built_in_acp_agents, AcpAgentId};

    fn every_preset() -> Vec<AgentPreset> {
        AgentPreset::listed(&built_in_acp_agents()).collect()
    }

    fn acp(id: &str) -> AgentPreset {
        AgentPreset::Acp(
            id.parse::<AcpAgentId>()
                .unwrap_or_else(|_| unreachable!("{id} is an id")),
        )
    }

    const fn expected_terminal_chat(preset: AgentPreset) -> Option<TerminalChat> {
        match preset {
            AgentPreset::ClaudeCode => Some(TerminalChat::InPlace),
            AgentPreset::Codex | AgentPreset::Opencode => Some(TerminalChat::Reopened),
            AgentPreset::Shell | AgentPreset::Gateway | AgentPreset::Acp(_) => None,
        }
    }

    const fn expected_structured_chat(preset: AgentPreset) -> bool {
        match preset {
            AgentPreset::ClaudeCode
            | AgentPreset::Codex
            | AgentPreset::Opencode
            | AgentPreset::Gateway
            | AgentPreset::Acp(_) => true,
            AgentPreset::Shell => false,
        }
    }

    #[test]
    fn every_agent_harness_chats_and_only_its_terminal_shows_a_chat() {
        for preset in every_preset() {
            assert_eq!(
                preset.has_structured_chat(),
                expected_structured_chat(preset),
                "{preset:?}"
            );
            assert_eq!(
                preset.terminal_chat(),
                expected_terminal_chat(preset),
                "{preset:?}"
            );
        }
    }

    #[test]
    fn a_new_chat_prefers_claude_code_then_opencode_codex_the_gateway_and_acp_agents() {
        assert_eq!(
            AgentPreset::chat_harnesses(&[]),
            [
                AgentPreset::ClaudeCode,
                AgentPreset::Opencode,
                AgentPreset::Codex,
                AgentPreset::Gateway
            ]
        );
        assert_eq!(
            AgentPreset::chat_harnesses(&built_in_acp_agents()),
            [
                AgentPreset::ClaudeCode,
                AgentPreset::Opencode,
                AgentPreset::Codex,
                AgentPreset::Gateway,
                acp("claude-code-acp"),
                acp("codex-acp"),
                acp("opencode"),
            ]
        );
    }

    #[test]
    fn an_acp_agent_opens_as_a_terminal_only_when_it_names_a_terminal_command() {
        let mut agents = built_in_acp_agents();
        assert_eq!(acp("codex-acp").terminal_command(&agents), Some("codex"));
        assert_eq!(AgentPreset::Codex.terminal_command(&agents), Some("codex"));
        assert_eq!(AgentPreset::Gateway.terminal_command(&agents), None);
        assert_eq!(acp("gemini").terminal_command(&agents), None);
        for agent in &mut agents {
            agent.terminal = None;
        }
        assert_eq!(acp("codex-acp").terminal_command(&agents), None);
    }

    #[test]
    fn only_claude_code_reads_shared_skills_settings_models_and_account_usage() {
        for preset in every_preset() {
            let claude = match preset {
                AgentPreset::ClaudeCode => true,
                AgentPreset::Shell
                | AgentPreset::Codex
                | AgentPreset::Opencode
                | AgentPreset::Gateway
                | AgentPreset::Acp(_) => false,
            };
            assert_eq!(preset.reads_shared_skills(), claude, "{preset:?}");
            assert_eq!(preset.takes_settings_models(), claude, "{preset:?}");
            assert_eq!(preset.reports_account_usage(), claude, "{preset:?}");
            assert_eq!(!preset.offered_efforts().is_empty(), claude, "{preset:?}");
        }
    }

    #[test]
    fn only_the_gateway_harness_chats_through_the_gateway() {
        for preset in every_preset() {
            let gateway = match preset {
                AgentPreset::Gateway => true,
                AgentPreset::Shell
                | AgentPreset::ClaudeCode
                | AgentPreset::Codex
                | AgentPreset::Opencode
                | AgentPreset::Acp(_) => false,
            };
            assert_eq!(preset.chats_through_gateway(), gateway, "{preset:?}");
        }
    }

    #[test]
    fn only_opencode_attaches_its_terminal_to_a_server() {
        for preset in every_preset() {
            let attaches = match preset {
                AgentPreset::Opencode => true,
                AgentPreset::Shell
                | AgentPreset::ClaudeCode
                | AgentPreset::Codex
                | AgentPreset::Gateway
                | AgentPreset::Acp(_) => false,
            };
            assert_eq!(preset.attaches_to_server(), attaches, "{preset:?}");
        }
    }

    #[test]
    fn every_agent_worker_opens_as_a_chat() {
        for preset in every_preset() {
            let surface = match preset {
                AgentPreset::ClaudeCode
                | AgentPreset::Codex
                | AgentPreset::Opencode
                | AgentPreset::Acp(_) => WorkerSurface::Chat,
                AgentPreset::Shell | AgentPreset::Gateway => WorkerSurface::Terminal,
            };
            assert_eq!(preset.worker_surface(), surface, "{preset:?}");
        }
    }
}
