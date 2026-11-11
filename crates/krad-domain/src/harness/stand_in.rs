use serde::{Deserialize, Serialize};

use crate::acp::{acp_stand_in_for, acp_stand_in_of};
use crate::agent::AgentPreset;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OfferedTwin {
    #[default]
    Native,
    Acp,
}

impl OfferedTwin {
    #[must_use]
    pub fn is_native(&self) -> bool {
        *self == Self::Native
    }

    #[must_use]
    pub const fn flipped(self) -> Self {
        match self {
            Self::Native => Self::Acp,
            Self::Acp => Self::Native,
        }
    }
}

impl AgentPreset {
    #[must_use]
    pub fn native_twin(self) -> Self {
        self.acp_agent().and_then(acp_stand_in_for).unwrap_or(self)
    }

    fn offered_instead(self, twin: OfferedTwin) -> Option<Self> {
        match (twin, self.acp_agent()) {
            (OfferedTwin::Acp, None) => acp_stand_in_of(self),
            (OfferedTwin::Native, Some(id)) => acp_stand_in_for(id),
            (OfferedTwin::Acp, Some(_)) | (OfferedTwin::Native, None) => None,
        }
    }

    #[must_use]
    pub fn as_offered(self, available: &[Self], twin: OfferedTwin) -> Self {
        self.offered_instead(twin)
            .filter(|twin| available.contains(twin))
            .unwrap_or(self)
    }

    #[must_use]
    pub fn is_offered(self, available: &[Self], twin: OfferedTwin) -> bool {
        self.as_offered(available, twin) == self
    }

    #[must_use]
    pub fn offered(available: &[Self], twin: OfferedTwin) -> Vec<Self> {
        available
            .iter()
            .copied()
            .filter(|preset| preset.is_offered(available, twin))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claude_acp() -> AgentPreset {
        AgentPreset::Acp(
            "claude-code-acp"
                .parse()
                .unwrap_or_else(|_| unreachable!("a valid id")),
        )
    }

    #[test]
    fn the_stream_json_harness_is_offered_by_default_and_hides_its_acp_twin() {
        let available = [
            AgentPreset::Shell,
            AgentPreset::ClaudeCode,
            AgentPreset::Codex,
            claude_acp(),
        ];
        let twin = OfferedTwin::default();
        assert_eq!(
            AgentPreset::offered(&available, twin),
            [
                AgentPreset::Shell,
                AgentPreset::ClaudeCode,
                AgentPreset::Codex
            ]
        );
        assert_eq!(
            claude_acp().as_offered(&available, twin),
            AgentPreset::ClaudeCode
        );
    }

    #[test]
    fn claude_code_over_acp_is_offered_instead_once_switched_on() {
        let available = [
            AgentPreset::Shell,
            AgentPreset::ClaudeCode,
            AgentPreset::Codex,
            claude_acp(),
        ];
        let twin = OfferedTwin::Acp;
        assert_eq!(
            AgentPreset::offered(&available, twin),
            [AgentPreset::Shell, AgentPreset::Codex, claude_acp()]
        );
        assert_eq!(
            AgentPreset::ClaudeCode.as_offered(&available, twin),
            claude_acp()
        );
        assert_eq!(
            AgentPreset::Codex.as_offered(&available, twin),
            AgentPreset::Codex
        );
    }

    #[test]
    fn the_stream_json_harness_stays_offered_while_its_acp_twin_cannot_run() {
        let available = [AgentPreset::Shell, AgentPreset::ClaudeCode];
        assert_eq!(
            AgentPreset::offered(&available, OfferedTwin::Acp),
            available
        );
        assert_eq!(
            AgentPreset::ClaudeCode.as_offered(&available, OfferedTwin::Acp),
            AgentPreset::ClaudeCode
        );
    }

    #[test]
    fn both_claude_harnesses_wear_the_native_claude_code_brand() {
        assert_eq!(claude_acp().native_twin(), AgentPreset::ClaudeCode);
        assert_eq!(
            AgentPreset::ClaudeCode.native_twin(),
            AgentPreset::ClaudeCode
        );
        let codex_acp = AgentPreset::Acp(
            "codex-acp"
                .parse()
                .unwrap_or_else(|_| unreachable!("a valid id")),
        );
        assert_eq!(codex_acp.native_twin(), codex_acp);
    }
}
