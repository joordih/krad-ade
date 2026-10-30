use serde::{Deserialize, Serialize};

use crate::agent::AgentPreset;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RewindId(String);

impl RewindId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for RewindId {
    fn from(id: &str) -> Self {
        Self(id.to_owned())
    }
}

impl From<String> for RewindId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RewindPoint {
    Message(RewindId),
    Turn(RewindId),
    Prompt(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RewindScope {
    Conversation,
    ConversationAndFiles,
}

impl RewindScope {
    #[must_use]
    pub const fn restores_files(self) -> bool {
        matches!(self, Self::ConversationAndFiles)
    }
}

impl AgentPreset {
    #[must_use]
    pub const fn restores_files(self) -> bool {
        match self {
            Self::ClaudeCode | Self::Opencode | Self::Gateway => true,
            Self::Shell | Self::Codex => false,
        }
    }

    #[must_use]
    pub const fn rewinds_from_home(self) -> bool {
        match self {
            Self::ClaudeCode => true,
            Self::Shell | Self::Codex | Self::Opencode | Self::Gateway => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_rewinds_without_its_files_and_only_claude_code_reads_them_from_home() {
        for preset in AgentPreset::all() {
            let (restores, from_home) = match preset {
                AgentPreset::ClaudeCode => (true, true),
                AgentPreset::Opencode | AgentPreset::Gateway => (true, false),
                AgentPreset::Shell | AgentPreset::Codex => (false, false),
            };
            assert_eq!(preset.restores_files(), restores, "{preset:?}");
            assert_eq!(preset.rewinds_from_home(), from_home, "{preset:?}");
        }
    }

    #[test]
    fn rewind_points_persist_as_tagged_ids() {
        for (point, persisted) in [
            (
                RewindPoint::Message(RewindId::from("msg_1")),
                r#"{"Message":"msg_1"}"#,
            ),
            (
                RewindPoint::Turn(RewindId::from("turn-2")),
                r#"{"Turn":"turn-2"}"#,
            ),
            (RewindPoint::Prompt(3), r#"{"Prompt":3}"#),
        ] {
            assert_eq!(
                serde_json::to_string(&point).ok().as_deref(),
                Some(persisted)
            );
            assert_eq!(
                serde_json::from_str::<RewindPoint>(persisted).ok(),
                Some(point)
            );
        }
    }
}
