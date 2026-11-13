use krad_domain::{
    AgentPreset, EnvironmentName, HostPlatform, PromptAsker, PromptKind, TerminalId,
};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

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
    },
    Reconnecting {
        attempt: u32,
    },
    Disconnected,
    Failed {
        reason: String,
    },
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
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::event::EventKind;

    fn main() -> EnvironmentName {
        EnvironmentName::new("main".to_owned())
    }

    #[test]
    fn a_show_has_its_wire_shape_and_a_newer_verb_reads_as_unknown() -> serde_json::Result<()> {
        let command = Command::Host(HostCommand::Show {
            environment: main(),
        });
        let json = serde_json::to_string(&command)?;
        assert_eq!(
            json,
            r#"{"area":"host","verb":"show","environment":"main"}"#
        );
        assert_eq!(serde_json::from_str::<Command>(&json)?, command);
        let newer: Command = serde_json::from_str(r#"{"area":"host","verb":"forget"}"#)?;
        assert_eq!(newer, Command::Host(HostCommand::Unknown));
        Ok(())
    }

    #[test]
    fn an_answer_travels_but_never_prints() -> serde_json::Result<()> {
        let command = HostCommand::AnswerPrompt {
            prompt: PromptId(3),
            answer: Some(PromptAnswer::new("hunter2".to_owned())),
        };
        assert!(!format!("{command:?}").contains("hunter2"));
        let json = serde_json::to_string(&command)?;
        assert_eq!(
            json,
            r#"{"verb":"answer_prompt","prompt":3,"answer":"hunter2"}"#
        );
        assert_eq!(serde_json::from_str::<HostCommand>(&json)?, command);
        Ok(())
    }

    #[test]
    fn host_events_round_trip() -> serde_json::Result<()> {
        let events = [
            EventKind::HostChanged {
                environment: main(),
                state: HostState::Connected {
                    platform: HostPlatform::LinuxX86_64,
                },
            },
            EventKind::HostChanged {
                environment: main(),
                state: HostState::Failed {
                    reason: "refused".to_owned(),
                },
            },
            EventKind::SignInWanted {
                environment: main(),
            },
            EventKind::MachineChannelWanted {
                environment: main(),
            },
            EventKind::RemoteBusChanged {
                environment: main(),
                bus: RemoteBusState::Ready(RemoteBusInfo {
                    remote_port: 41_000,
                    bin_dir: PathBuf::from("/home/me/.krad/bin"),
                    token_file: PathBuf::from("/home/me/.krad/bus/main.token"),
                    endpoint_file: PathBuf::from("/home/me/.krad/bus/main.endpoint"),
                    claude_settings: PathBuf::from("/home/me/.krad/claude-hooks.json"),
                }),
            },
            EventKind::HostPrompted(HostPrompt {
                prompt: PromptId(1),
                asker: PromptAsker::Git,
                kind: PromptKind::Secret,
                text: "Password: ".to_owned(),
            }),
            EventKind::HostPromptSettled {
                prompt: PromptId(1),
            },
        ];
        for event in events {
            let json = serde_json::to_string(&event)?;
            assert_eq!(serde_json::from_str::<EventKind>(&json)?, event);
        }
        Ok(())
    }

    #[test]
    fn only_a_connected_host_is_ready_and_only_a_lost_one_reconnects() {
        let connected = HostState::Connected {
            platform: HostPlatform::LinuxAarch64,
        };
        assert!(connected.is_connected());
        assert!(!connected.can_reconnect());
        assert!(HostState::NotShown.connects_when_shown());
        assert!(!HostState::Disconnected.connects_when_shown());
        assert!(HostState::Reconnecting { attempt: 2 }.can_reconnect());
        assert!(HostState::Reconnecting { attempt: 2 }.is_pending());
        assert_eq!(
            HostState::SigningIn {
                terminal: TerminalId(4)
            }
            .sign_in_terminal(),
            Some(TerminalId(4))
        );
        assert!(RemoteBusState::Preparing.is_preparing());
        assert!(RemoteBusState::Preparing.ready().is_none());
    }

    #[test]
    fn local_agents_are_detected_per_place_and_read_from_older_snapshots() -> serde_json::Result<()>
    {
        let detect = Command::Host(HostCommand::DetectLocalAgents {
            place: AgentPlace::Wsl {
                distro: "Ubuntu".to_owned(),
            },
        });
        let json = serde_json::to_string(&detect)?;
        assert_eq!(
            json,
            r#"{"area":"host","verb":"detect_local_agents","place":{"place":"wsl","distro":"Ubuntu"}}"#
        );
        assert_eq!(serde_json::from_str::<Command>(&json)?, detect);
        let found = EventKind::LocalAgentsFound(LocalAgents {
            place: AgentPlace::Native,
            agents: vec![AgentPreset::ClaudeCode],
        });
        let json = serde_json::to_string(&found)?;
        assert_eq!(serde_json::from_str::<EventKind>(&json)?, found);
        let older: HostsSnapshot = serde_json::from_str(r#"{"hosts":[],"prompts":[]}"#)?;
        assert_eq!(older.local, Vec::new());
        Ok(())
    }
}
