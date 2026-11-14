use krad_domain::{ClaudeOauthToken, CloudRunnerId, RunnerCli, RunnerLogin, RunnerState};
use serde::{Deserialize, Serialize};

use crate::hosts::RemoteBusState;

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
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
    CheckLogins {
        runner: CloudRunnerId,
    },
    KeepClaudeToken {
        runner: CloudRunnerId,
        token: ClaudeOauthToken,
    },
    ForgetClaudeToken {
        runner: CloudRunnerId,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct CliLogin {
    pub cli: RunnerCli,
    pub login: RunnerLogin,
}

#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct RunnerLogins {
    pub runner: CloudRunnerId,
    pub logins: Vec<CliLogin>,
    pub claude_token: bool,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "news", rename_all = "snake_case")]
pub enum RunnerNews {
    ClaudeTokenKept,
    ClaudeTokenForgotten,
    ClaudeTokenNotKept {
        error: String,
    },
    ClaudeTokenNotForgotten {
        error: String,
    },
    SignInForwarded {
        url: String,
    },
    SignInNotForwarded {
        message: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct RunnersSnapshot {
    #[serde(default)]
    pub shown: Vec<RunnerShown>,
    #[serde(default)]
    pub buses: Vec<RunnerBusShown>,
    #[serde(default)]
    pub logins: Vec<RunnerLogins>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RunnerNotice {
    pub runner: CloudRunnerId,
    pub news: RunnerNews,
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
    fn logins_and_news_round_trip_without_the_token() -> serde_json::Result<()> {
        let logins = RunnerLogins {
            runner: CloudRunnerId(3),
            logins: vec![CliLogin {
                cli: RunnerCli::Codex,
                login: RunnerLogin::SignedOut,
            }],
            claude_token: true,
        };
        let json = serde_json::to_string(&logins)?;
        assert_eq!(serde_json::from_str::<RunnerLogins>(&json)?, logins);
        let notice = RunnerNotice {
            runner: CloudRunnerId(3),
            news: RunnerNews::SignInForwarded {
                url: "http://localhost:1455/callback".to_owned(),
            },
        };
        let json = serde_json::to_string(&notice)?;
        assert_eq!(serde_json::from_str::<RunnerNotice>(&json)?, notice);
        let token: ClaudeOauthToken = "sk-ant-oat01-abc"
            .parse()
            .map_err(serde::de::Error::custom)?;
        let keep = RunnerCommand::KeepClaudeToken {
            runner: CloudRunnerId(3),
            token,
        };
        assert!(!format!("{keep:?}").contains("sk-ant"));
        Ok(())
    }

    #[test]
    fn a_runner_bus_round_trips() -> serde_json::Result<()> {
        let bus = RunnerBusShown {
            runner: CloudRunnerId(3),
            bus: RemoteBusState::Preparing,
        };
        let json = serde_json::to_string(&bus)?;
        assert_eq!(serde_json::from_str::<RunnerBusShown>(&json)?, bus);
        Ok(())
    }
}
