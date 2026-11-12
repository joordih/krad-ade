use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "area", rename_all = "snake_case")]
pub enum Command {
    Runtime(RuntimeCommand),
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
        let area: Command = serde_json::from_str(r#"{"area":"kraddie","verb":"run"}"#)?;
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
