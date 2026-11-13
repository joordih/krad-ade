use serde::{Deserialize, Serialize};

use crate::AgentPreset;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunnerCli {
    Claude,
    Codex,
    Opencode,
    Github,
    Gitlab,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunnerLogin {
    Checking,
    SignedIn,
    SignedOut,
    Missing,
    Unknown,
}

impl RunnerCli {
    pub const ALL: [Self; 5] = [
        Self::Claude,
        Self::Codex,
        Self::Opencode,
        Self::Github,
        Self::Gitlab,
    ];

    #[must_use]
    pub const fn harness(self) -> Option<AgentPreset> {
        match self {
            Self::Claude => Some(AgentPreset::ClaudeCode),
            Self::Codex => Some(AgentPreset::Codex),
            Self::Opencode => Some(AgentPreset::Opencode),
            Self::Github | Self::Gitlab => None,
        }
    }

    #[must_use]
    pub fn of_harness(harness: AgentPreset) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|cli| cli.harness() == Some(harness))
    }
}

impl RunnerLogin {
    #[must_use]
    pub const fn needs_sign_in(self) -> bool {
        matches!(self, Self::SignedOut)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_harness_signs_in_with_its_own_cli() {
        assert_eq!(
            RunnerCli::of_harness(AgentPreset::Codex),
            Some(RunnerCli::Codex)
        );
        assert_eq!(RunnerCli::of_harness(AgentPreset::Shell), None);
        assert!(RunnerLogin::SignedOut.needs_sign_in());
        assert!(!RunnerLogin::Missing.needs_sign_in());
    }

    #[test]
    fn a_cli_and_its_login_round_trip() -> serde_json::Result<()> {
        let json = serde_json::to_string(&(RunnerCli::Github, RunnerLogin::SignedOut))?;
        assert_eq!(json, r#"["github","signed_out"]"#);
        assert_eq!(
            serde_json::from_str::<(RunnerCli, RunnerLogin)>(&json)?,
            (RunnerCli::Github, RunnerLogin::SignedOut)
        );
        Ok(())
    }
}
