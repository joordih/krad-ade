use krad_domain::{AgentState, AttentionKind, AttentionTarget, Sight, Timestamp};
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum AttentionCommand {
    Observe {
        target: AttentionTarget,
        previous: AgentState,
        next: AgentState,
        waiting_for: AttentionKind,
        sight: Sight,
        title: String,
    },
    Raise {
        target: AttentionTarget,
        kind: AttentionKind,
        sight: Sight,
        title: String,
    },
    Seen {
        target: AttentionTarget,
    },
    Dismiss {
        target: AttentionTarget,
    },
    ClearHistory,
    #[cfg_attr(feature = "ts", ts(skip))]
    #[serde(other)]
    Unknown,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AttentionNotice {
    pub target: AttentionTarget,
    pub kind: AttentionKind,
    pub title: String,
    pub at: Timestamp,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AttentionWaiting {
    pub target: AttentionTarget,
    pub kind: AttentionKind,
    pub since: Timestamp,
}

#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AttentionSnapshot {
    #[serde(default)]
    pub waiting: Vec<AttentionWaiting>,
    #[serde(default)]
    pub history: Vec<AttentionNotice>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Event, EventKind, Seq};
    use krad_domain::{EnvironmentName, TerminalId};
    use std::time::UNIX_EPOCH;

    fn target() -> AttentionTarget {
        AttentionTarget::new(EnvironmentName::new("main".to_owned()), TerminalId(2))
    }

    #[test]
    fn a_seen_command_has_a_stable_wire_shape() -> serde_json::Result<()> {
        let command = Command::Attention(AttentionCommand::Seen { target: target() });
        let json = serde_json::to_string(&command)?;
        assert_eq!(
            json,
            r#"{"area":"attention","verb":"seen","target":{"environment":"main","terminal":2}}"#
        );
        assert_eq!(serde_json::from_str::<Command>(&json)?, command);
        let newer: Command = serde_json::from_str(r#"{"area":"attention","verb":"snooze"}"#)?;
        assert_eq!(newer, Command::Attention(AttentionCommand::Unknown));
        Ok(())
    }

    #[test]
    fn attention_events_round_trip() -> serde_json::Result<()> {
        let notice = AttentionNotice {
            target: target(),
            kind: AttentionKind::Permission,
            title: "claude".to_owned(),
            at: Timestamp::from(UNIX_EPOCH),
        };
        let events = [
            EventKind::AttentionRaised(notice),
            EventKind::AttentionLeft { target: target() },
            EventKind::AttentionHistoryCleared,
        ];
        for kind in events {
            let event = Event { seq: Seq(1), kind };
            let json = serde_json::to_string(&event)?;
            assert_eq!(serde_json::from_str::<Event>(&json)?, event);
        }
        Ok(())
    }
}
