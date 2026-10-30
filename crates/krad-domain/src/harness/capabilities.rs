use crate::agent::AgentPreset;
use crate::model::Effort;

pub const DEFAULT_CHAT_HARNESS: AgentPreset = AgentPreset::ClaudeCode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalChat {
    InPlace,
    Reopened,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ChatPreference {
    First,
    Second,
    Third,
    Fourth,
}

impl AgentPreset {
    const fn chat_preference(self) -> Option<ChatPreference> {
        match self {
            Self::ClaudeCode => Some(ChatPreference::First),
            Self::Opencode => Some(ChatPreference::Second),
            Self::Codex => Some(ChatPreference::Third),
            Self::Gateway => Some(ChatPreference::Fourth),
            Self::Shell => None,
        }
    }

    #[must_use]
    pub const fn has_structured_chat(self) -> bool {
        self.chat_preference().is_some()
    }

    #[must_use]
    pub fn chat_harnesses() -> Vec<Self> {
        let mut harnesses: Vec<Self> = Self::all()
            .into_iter()
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
            Self::Shell | Self::Gateway => None,
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
            Self::Shell | Self::Codex | Self::Opencode | Self::Gateway => false,
        }
    }

    #[must_use]
    pub const fn offered_efforts(self) -> &'static [Effort] {
        match self {
            Self::ClaudeCode => &Effort::ALL,
            Self::Shell | Self::Codex | Self::Opencode | Self::Gateway => &[],
        }
    }

    #[must_use]
    pub const fn takes_settings_models(self) -> bool {
        match self {
            Self::ClaudeCode => true,
            Self::Shell | Self::Codex | Self::Opencode | Self::Gateway => false,
        }
    }

    #[must_use]
    pub const fn reports_account_usage(self) -> bool {
        match self {
            Self::ClaudeCode => true,
            Self::Shell | Self::Codex | Self::Opencode | Self::Gateway => false,
        }
    }

    #[must_use]
    pub const fn chats_through_gateway(self) -> bool {
        match self {
            Self::Gateway => true,
            Self::Shell | Self::ClaudeCode | Self::Codex | Self::Opencode => false,
        }
    }

    #[must_use]
    pub const fn attaches_to_server(self) -> bool {
        match self {
            Self::Opencode => true,
            Self::Shell | Self::ClaudeCode | Self::Codex | Self::Gateway => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn expected_terminal_chat(preset: AgentPreset) -> Option<TerminalChat> {
        match preset {
            AgentPreset::ClaudeCode => Some(TerminalChat::InPlace),
            AgentPreset::Codex | AgentPreset::Opencode => Some(TerminalChat::Reopened),
            AgentPreset::Shell | AgentPreset::Gateway => None,
        }
    }

    const fn expected_structured_chat(preset: AgentPreset) -> bool {
        match preset {
            AgentPreset::ClaudeCode
            | AgentPreset::Codex
            | AgentPreset::Opencode
            | AgentPreset::Gateway => true,
            AgentPreset::Shell => false,
        }
    }

    #[test]
    fn every_agent_harness_chats_and_only_its_terminal_shows_a_chat() {
        for preset in AgentPreset::all() {
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
    fn a_new_chat_prefers_claude_code_then_opencode_codex_and_the_gateway() {
        assert_eq!(
            AgentPreset::chat_harnesses(),
            [
                AgentPreset::ClaudeCode,
                AgentPreset::Opencode,
                AgentPreset::Codex,
                AgentPreset::Gateway
            ]
        );
    }

    #[test]
    fn only_claude_code_reads_shared_skills_settings_models_and_account_usage() {
        for preset in AgentPreset::all() {
            let claude = match preset {
                AgentPreset::ClaudeCode => true,
                AgentPreset::Shell
                | AgentPreset::Codex
                | AgentPreset::Opencode
                | AgentPreset::Gateway => false,
            };
            assert_eq!(preset.reads_shared_skills(), claude, "{preset:?}");
            assert_eq!(preset.takes_settings_models(), claude, "{preset:?}");
            assert_eq!(preset.reports_account_usage(), claude, "{preset:?}");
            assert_eq!(!preset.offered_efforts().is_empty(), claude, "{preset:?}");
        }
    }

    #[test]
    fn only_the_gateway_harness_chats_through_the_gateway() {
        for preset in AgentPreset::all() {
            let gateway = match preset {
                AgentPreset::Gateway => true,
                AgentPreset::Shell
                | AgentPreset::ClaudeCode
                | AgentPreset::Codex
                | AgentPreset::Opencode => false,
            };
            assert_eq!(preset.chats_through_gateway(), gateway, "{preset:?}");
        }
    }

    #[test]
    fn only_opencode_attaches_its_terminal_to_a_server() {
        for preset in AgentPreset::all() {
            let attaches = match preset {
                AgentPreset::Opencode => true,
                AgentPreset::Shell
                | AgentPreset::ClaudeCode
                | AgentPreset::Codex
                | AgentPreset::Gateway => false,
            };
            assert_eq!(preset.attaches_to_server(), attaches, "{preset:?}");
        }
    }
}
