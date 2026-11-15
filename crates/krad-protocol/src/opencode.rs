use krad_domain::Host;
use serde::{Deserialize, Serialize};

use crate::spots::HarnessSpot;

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum OpencodeStatus {
    Starting,
    Ready,
    Failed { error: String },
    Stopped,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct OpencodeShown {
    pub host: Host,
    pub status: OpencodeStatus,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum OpencodeCommand {
    Start {
        #[serde(default)]
        spot: HarnessSpot,
    },
    Refresh {
        host: Host,
    },
    #[cfg_attr(feature = "ts", ts(skip))]
    #[serde(other)]
    Unknown,
}

#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct OpencodeSnapshot {
    #[serde(default)]
    pub servers: Vec<OpencodeShown>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Event, EventKind, Seq};

    #[test]
    fn an_opencode_server_event_has_a_stable_wire_shape() -> serde_json::Result<()> {
        let event = Event {
            seq: Seq(4),
            kind: EventKind::OpencodeServer(OpencodeShown {
                host: Host::Local,
                status: OpencodeStatus::Failed {
                    error: "no opencode".to_owned(),
                },
            }),
        };
        let json = serde_json::to_string(&event)?;
        assert_eq!(
            json,
            r#"{"seq":4,"kind":{"type":"opencode_server","host":{"kind":"local"},"status":{"state":"failed","error":"no opencode"}}}"#
        );
        assert_eq!(serde_json::from_str::<Event>(&json)?, event);
        Ok(())
    }
}
