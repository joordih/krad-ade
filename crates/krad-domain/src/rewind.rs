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
        matches!(self, Self::ClaudeCode | Self::Opencode | Self::Gateway)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_codex_rewinds_without_its_files() {
        let restoring: Vec<AgentPreset> = AgentPreset::all()
            .into_iter()
            .filter(|preset| preset.restores_files())
            .collect();
        assert_eq!(
            restoring,
            [
                AgentPreset::ClaudeCode,
                AgentPreset::Opencode,
                AgentPreset::Gateway
            ]
        );
    }

    #[test]
    fn rewind_points_round_trip_through_json() {
        for point in [
            RewindPoint::Message(RewindId::from("msg_1")),
            RewindPoint::Turn(RewindId::from("turn-2")),
            RewindPoint::Prompt(3),
        ] {
            let line = serde_json::to_string(&point).unwrap_or_default();
            assert_eq!(serde_json::from_str::<RewindPoint>(&line).ok(), Some(point));
        }
    }
}
