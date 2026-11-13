use serde::{Deserialize, Serialize};

use crate::app_events::EventsCommand;
use crate::attention::AttentionCommand;
use crate::bus::BusCommand;
use crate::chats::ChatCommand;
use crate::environments::EnvironmentCommand;
use crate::gateway::GatewayCommand;
use crate::git::{ChangesCommand, GitCommand};
use crate::hosts::HostCommand;
use crate::kraddies::KraddieCommand;
use crate::models::ModelsCommand;
use crate::sessions::{GroupCommand, NodeCommand, SessionCommand};
use crate::settings::SettingsCommand;
use crate::terminals::TerminalCommand;
use crate::usage::UsageCommand;

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "area", rename_all = "snake_case")]
pub enum Command {
    Runtime(RuntimeCommand),
    Bus(BusCommand),
    Events(EventsCommand),
    Attention(AttentionCommand),
    Usage(UsageCommand),
    Models(ModelsCommand),
    Environments(EnvironmentCommand),
    Session(SessionCommand),
    Group(GroupCommand),
    Node(NodeCommand),
    Terminal(TerminalCommand),
    Host(HostCommand),
    Chat(ChatCommand),
    Gateway(GatewayCommand),
    Settings(SettingsCommand),
    Git(GitCommand),
    Changes(ChangesCommand),
    Kraddie(KraddieCommand),
    #[serde(other)]
    Unknown,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum RuntimeCommand {
    Ping,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Reply {
    Accepted,
    Refused { code: ErrorCode, message: String },
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Unsupported,
    NotFound,
    AlreadyAnswered,
    Invalid,
    Unavailable,
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_round_trips_through_json() -> serde_json::Result<()> {
        let command = Command::Runtime(RuntimeCommand::Ping);
        let json = serde_json::to_string(&command)?;
        assert_eq!(json, r#"{"area":"runtime","verb":"ping"}"#);
        assert_eq!(serde_json::from_str::<Command>(&json)?, command);
        Ok(())
    }

    #[test]
    fn a_command_from_a_newer_client_reads_as_unknown() -> serde_json::Result<()> {
        let area: Command = serde_json::from_str(r#"{"area":"variant","verb":"run"}"#)?;
        assert_eq!(area, Command::Unknown);
        let verb: Command = serde_json::from_str(r#"{"area":"runtime","verb":"restart"}"#)?;
        assert_eq!(verb, Command::Runtime(RuntimeCommand::Unknown));
        Ok(())
    }

    #[test]
    fn a_refusal_keeps_its_stable_code() -> serde_json::Result<()> {
        let reply = Reply::Refused {
            code: ErrorCode::Unsupported,
            message: "no".to_owned(),
        };
        let json = serde_json::to_string(&reply)?;
        assert_eq!(
            json,
            r#"{"outcome":"refused","code":"unsupported","message":"no"}"#
        );
        assert_eq!(serde_json::from_str::<Reply>(&json)?, reply);
        let newer: Reply =
            serde_json::from_str(r#"{"outcome":"refused","code":"rate_limited","message":"m"}"#)?;
        assert!(matches!(
            newer,
            Reply::Refused {
                code: ErrorCode::Unknown,
                ..
            }
        ));
        Ok(())
    }
}
