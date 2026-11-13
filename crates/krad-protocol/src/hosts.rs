use krad_domain::{
    AcpAgentId, AgentPreset, EnvironmentName, HostPlatform, ManagedAdapter, PromptAsker,
    PromptKind, TerminalId,
};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

use crate::spots::HarnessSpot;

const REDACTED: &str = "[REDACTED]";

#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum HostState {
    #[default]
    NotShown,
    Connecting,
    SigningIn {
        terminal: TerminalId,
    },
    SignedIn,
    Connected {
        platform: HostPlatform,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        home: Option<PathBuf>,
        #[serde(default)]
        login_env: LoginEnv,
    },
    Reconnecting {
        attempt: u32,
    },
    Disconnected,
    Failed {
        reason: String,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoginEnv {
    #[default]
    Read,
    Missing,
}

impl HostState {
    #[must_use]
    pub const fn connects_when_shown(&self) -> bool {
        matches!(self, Self::NotShown)
    }

    #[must_use]
    pub const fn is_connected(&self) -> bool {
        matches!(self, Self::Connected { .. })
    }

    #[must_use]
    pub const fn can_reconnect(&self) -> bool {
        matches!(
            self,
            Self::Disconnected | Self::Failed { .. } | Self::Reconnecting { .. }
        )
    }

    #[must_use]
    pub const fn is_pending(&self) -> bool {
        matches!(
            self,
            Self::NotShown
                | Self::Connecting
                | Self::SigningIn { .. }
                | Self::SignedIn
                | Self::Reconnecting { .. }
        )
    }

    #[must_use]
    pub const fn sign_in_terminal(&self) -> Option<TerminalId> {
        match self {
            Self::SigningIn { terminal } => Some(*terminal),
            _ => None,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RemoteBusInfo {
    pub remote_port: u16,
    pub bin_dir: PathBuf,
    pub token_file: PathBuf,
    pub endpoint_file: PathBuf,
    pub claude_settings: PathBuf,
}

#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(tag = "bus", rename_all = "snake_case")]
pub enum RemoteBusState {
    #[default]
    Absent,
    Preparing,
    Ready(RemoteBusInfo),
    Unavailable {
        reason: String,
    },
}

impl RemoteBusState {
    #[must_use]
    pub const fn ready(&self) -> Option<&RemoteBusInfo> {
        match self {
            Self::Ready(bus) => Some(bus),
            Self::Absent | Self::Preparing | Self::Unavailable { .. } => None,
        }
    }

    #[must_use]
    pub const fn is_preparing(&self) -> bool {
        matches!(self, Self::Preparing)
    }
}

#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default, Serialize, Deserialize,
)]
pub struct PromptId(pub u64);

impl PromptId {
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct HostPrompt {
    pub prompt: PromptId,
    pub asker: PromptAsker,
    pub kind: PromptKind,
    pub text: String,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PromptAnswer(String);

impl PromptAnswer {
    #[must_use]
    pub const fn new(text: String) -> Self {
        Self(text)
    }

    #[must_use]
    pub fn into_text(self) -> String {
        self.0
    }
}

impl fmt::Debug for PromptAnswer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(REDACTED)
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct EnvironmentHost {
    pub environment: EnvironmentName,
    pub state: HostState,
    #[serde(default)]
    pub bus: RemoteBusState,
    #[serde(default)]
    pub agents: Vec<AgentPreset>,
}

#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct HostsSnapshot {
    #[serde(default)]
    pub hosts: Vec<EnvironmentHost>,
    #[serde(default)]
    pub prompts: Vec<HostPrompt>,
    #[serde(default)]
    pub local: Vec<LocalAgents>,
    #[serde(default)]
    pub adapters: Vec<KeptAdapter>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct KeptAdapter {
    pub agent: AcpAgentId,
    pub adapter: ManagedAdapter,
}

#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Serialize, Deserialize)]
#[serde(tag = "place", rename_all = "snake_case")]
pub enum AgentPlace {
    Native,
    Wsl { distro: String },
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct LocalAgents {
    pub place: AgentPlace,
    pub agents: Vec<AgentPreset>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum HostCommand {
    Show {
        environment: EnvironmentName,
    },
    Reconnect {
        environment: EnvironmentName,
    },
    AnswerPrompt {
        prompt: PromptId,
        answer: Option<PromptAnswer>,
    },
    DetectAgents {
        environment: EnvironmentName,
    },
    DetectLocalAgents {
        place: AgentPlace,
    },
    CheckAcpAdapter {
        agent: AcpAgentId,
    },
    RetryAcpAdapter {
        agent: AcpAgentId,
    },
    CloseAll,
    SshHosts,
    AcpPackages {
        spot: HarnessSpot,
    },
    InstallAcpPackage {
        spot: HarnessSpot,
        agent: AcpAgentId,
    },
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
mod tests;
