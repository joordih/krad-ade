use crate::acp::{acp_stand_in_for, acp_stand_in_of};
use crate::agent::AgentPreset;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OfferedTwin {
    Native,
    Acp,
}

pub const OFFERED_TWIN: OfferedTwin = OfferedTwin::Acp;

impl AgentPreset {
    #[must_use]
    pub fn native_twin(self) -> Self {
        self.acp_agent().and_then(acp_stand_in_for).unwrap_or(self)
    }

    fn offered_instead(self) -> Option<Self> {
        match (OFFERED_TWIN, self.acp_agent()) {
            (OfferedTwin::Acp, None) => acp_stand_in_of(self),
            (OfferedTwin::Native, Some(id)) => acp_stand_in_for(id),
            (OfferedTwin::Acp, Some(_)) | (OfferedTwin::Native, None) => None,
        }
    }

    #[must_use]
    pub fn as_offered(self, available: &[Self]) -> Self {
        self.offered_instead()
            .filter(|twin| available.contains(twin))
            .unwrap_or(self)
    }

    #[must_use]
    pub fn is_offered(self, available: &[Self]) -> bool {
        self.as_offered(available) == self
    }

    #[must_use]
    pub fn offered(available: &[Self]) -> Vec<Self> {
        available
            .iter()
            .copied()
            .filter(|preset| preset.is_offered(available))
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
    fn claude_code_over_acp_is_offered_instead_of_the_stream_json_harness() {
        let available = [
            AgentPreset::Shell,
            AgentPreset::ClaudeCode,
            AgentPreset::Codex,
            claude_acp(),
        ];
        assert_eq!(
            AgentPreset::offered(&available),
            [AgentPreset::Shell, AgentPreset::Codex, claude_acp()]
        );
        assert_eq!(AgentPreset::ClaudeCode.as_offered(&available), claude_acp());
        assert_eq!(
            AgentPreset::Codex.as_offered(&available),
            AgentPreset::Codex
        );
    }

    #[test]
    fn the_stream_json_harness_stays_offered_while_its_acp_twin_cannot_run() {
        let available = [AgentPreset::Shell, AgentPreset::ClaudeCode];
        assert_eq!(AgentPreset::offered(&available), available);
        assert_eq!(
            AgentPreset::ClaudeCode.as_offered(&available),
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
