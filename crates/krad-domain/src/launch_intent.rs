use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::{AcpAgentId, AgentPreset, ModelId, ProjectTask, RelaunchIntent, ResumeId};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "intent", rename_all = "snake_case")]
pub enum LaunchIntent {
    Preset {
        preset: AgentPreset,
    },
    Agent {
        preset: AgentPreset,
        prompt: String,
        model: Option<ModelId>,
    },
    Resume {
        preset: AgentPreset,
        resume: Option<ResumeId>,
    },
    Task {
        task: ProjectTask,
    },
    AcpSignIn {
        agent: AcpAgentId,
        program: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: BTreeMap<String, String>,
    },
}

impl LaunchIntent {
    #[must_use]
    pub const fn harness(&self) -> Option<AgentPreset> {
        match self {
            Self::Preset { preset } | Self::Agent { preset, .. } | Self::Resume { preset, .. }
                if preset.is_agent() =>
            {
                Some(*preset)
            }
            Self::Preset { .. }
            | Self::Agent { .. }
            | Self::Resume { .. }
            | Self::Task { .. }
            | Self::AcpSignIn { .. } => None,
        }
    }

    #[must_use]
    pub fn model(&self) -> Option<&ModelId> {
        match self {
            Self::Agent { model, .. } => model.as_ref(),
            Self::Preset { .. }
            | Self::Resume { .. }
            | Self::Task { .. }
            | Self::AcpSignIn { .. } => None,
        }
    }
}

impl From<RelaunchIntent> for LaunchIntent {
    fn from(intent: RelaunchIntent) -> Self {
        match intent {
            RelaunchIntent::Shell => Self::Preset {
                preset: AgentPreset::Shell,
            },
            RelaunchIntent::Resume { preset, resume } => Self::Resume { preset, resume },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_agent_presets_launch_a_harness() {
        let shell: LaunchIntent = RelaunchIntent::Shell.into();
        assert_eq!(shell.harness(), None);
        let resumed: LaunchIntent = RelaunchIntent::Resume {
            preset: AgentPreset::ClaudeCode,
            resume: None,
        }
        .into();
        assert_eq!(resumed.harness(), Some(AgentPreset::ClaudeCode));
    }

    #[test]
    fn a_launch_intent_has_a_stable_wire_shape() -> serde_json::Result<()> {
        let intent = LaunchIntent::Agent {
            preset: AgentPreset::ClaudeCode,
            prompt: "fix it".to_owned(),
            model: None,
        };
        let json = serde_json::to_string(&intent)?;
        assert!(json.starts_with(r#"{"intent":"agent","preset":"#));
        assert_eq!(serde_json::from_str::<LaunchIntent>(&json)?, intent);
        Ok(())
    }
}
