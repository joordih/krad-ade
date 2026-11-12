use krad_domain::{AgentPreset, AgentStatus, Host, LimitAlert, PlanUsage, UsageReading};
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageNote {
    Starting,
    HostNotReady,
    Broken(String),
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "news", rename_all = "snake_case")]
pub enum UsageNews {
    Read { usage: PlanUsage },
    Alert { alert: Option<LimitAlert> },
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum UsageCommand {
    Refresh,
    Heard {
        host: Host,
        harness: AgentPreset,
        news: UsageNews,
    },
    Statusline {
        host: Host,
        status: AgentStatus,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct UsageShown {
    pub host: Host,
    pub harness: AgentPreset,
    #[serde(default)]
    pub reading: Option<UsageReading>,
    #[serde(default)]
    pub note: Option<UsageNote>,
}

#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct UsageSnapshot {
    #[serde(default)]
    pub shown: Vec<UsageShown>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Event, EventKind, Seq};
    use krad_domain::{UsageFailure, UsageWindow, WindowSpan};

    #[test]
    fn a_refresh_has_a_stable_wire_shape() -> serde_json::Result<()> {
        let command = Command::Usage(UsageCommand::Refresh);
        let json = serde_json::to_string(&command)?;
        assert_eq!(json, r#"{"area":"usage","verb":"refresh"}"#);
        assert_eq!(serde_json::from_str::<Command>(&json)?, command);
        let newer: Command = serde_json::from_str(r#"{"area":"usage","verb":"pause"}"#)?;
        assert_eq!(newer, Command::Usage(UsageCommand::Unknown));
        Ok(())
    }

    #[test]
    fn heard_usage_and_shown_readings_round_trip() -> serde_json::Result<()> {
        let usage = PlanUsage {
            windows: vec![UsageWindow {
                span: WindowSpan::FiveHours,
                scope: None,
                used: 30,
                resets_at: None,
                limited: false,
            }],
            ..PlanUsage::default()
        };
        let command = Command::Usage(UsageCommand::Heard {
            host: Host::Local,
            harness: AgentPreset::ClaudeCode,
            news: UsageNews::Read {
                usage: usage.clone(),
            },
        });
        let json = serde_json::to_string(&command)?;
        assert_eq!(serde_json::from_str::<Command>(&json)?, command);
        let event = Event {
            seq: Seq(2),
            kind: EventKind::UsageShown(UsageShown {
                host: Host::Local,
                harness: AgentPreset::Codex,
                reading: Some(UsageReading::Failed {
                    last: Some(usage),
                    reason: UsageFailure::Unreachable,
                }),
                note: Some(UsageNote::Broken("no runner".to_owned())),
            }),
        };
        let json = serde_json::to_string(&event)?;
        assert_eq!(serde_json::from_str::<Event>(&json)?, event);
        Ok(())
    }
}
