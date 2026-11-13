use krad_domain::{AttentionKind, CloudRunnerId, RunnerState, WebhookId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::bus::AgentRef;
use crate::hosts::RemoteBusState;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum RunnerCommand {
    Ensure {
        runner: CloudRunnerId,
    },
    Reconnect {
        runner: CloudRunnerId,
    },
    Remove {
        runner: CloudRunnerId,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RunnerShown {
    pub runner: CloudRunnerId,
    pub state: RunnerState,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RunnerBusShown {
    pub runner: CloudRunnerId,
    pub bus: RemoteBusState,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct CloudAskReplayed {
    pub agent: AgentRef,
    pub kind: AttentionKind,
    #[serde(default)]
    pub delivered: BTreeSet<WebhookId>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RunnerRemoved {
    pub runner: CloudRunnerId,
    pub problem: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::Linger;

    #[test]
    fn a_runner_command_and_its_state_round_trip() -> serde_json::Result<()> {
        let command = RunnerCommand::Ensure {
            runner: CloudRunnerId(3),
        };
        let json = serde_json::to_string(&command)?;
        assert_eq!(json, r#"{"verb":"ensure","runner":3}"#);
        assert_eq!(serde_json::from_str::<RunnerCommand>(&json)?, command);
        let shown = RunnerShown {
            runner: CloudRunnerId(3),
            state: RunnerState::Ready {
                linger: Linger::Unavailable,
            },
        };
        let json = serde_json::to_string(&shown)?;
        assert_eq!(serde_json::from_str::<RunnerShown>(&json)?, shown);
        assert_eq!(
            serde_json::from_str::<RunnerCommand>(r#"{"verb":"pair_again","runner":3}"#)?,
            RunnerCommand::Unknown
        );
        Ok(())
    }

    #[test]
    fn a_runner_bus_and_a_replayed_cloud_ask_round_trip() -> serde_json::Result<()> {
        let bus = RunnerBusShown {
            runner: CloudRunnerId(3),
            bus: RemoteBusState::Preparing,
        };
        let json = serde_json::to_string(&bus)?;
        assert_eq!(serde_json::from_str::<RunnerBusShown>(&json)?, bus);
        let asked = CloudAskReplayed {
            agent: AgentRef {
                environment: krad_domain::EnvironmentName::new("api".to_owned()),
                terminal: krad_domain::TerminalId(2),
            },
            kind: AttentionKind::Permission,
            delivered: [WebhookId(4)].into_iter().collect(),
        };
        let json = serde_json::to_string(&asked)?;
        assert_eq!(serde_json::from_str::<CloudAskReplayed>(&json)?, asked);
        Ok(())
    }
}
