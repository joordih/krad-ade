use krad_domain::{CloudRunnerId, RunnerState};
use serde::{Deserialize, Serialize};

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
}
