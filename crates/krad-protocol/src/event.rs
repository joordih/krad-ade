use serde::{Deserialize, Serialize};

use crate::bus::{AskId, AskSettlement, BusSnapshot, MailCount, PendingAsk, StatusReport};
use crate::ids::{RequestId, Seq};

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Event {
    pub seq: Seq,
    pub kind: EventKind,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    Pong {
        request: RequestId,
    },
    AgentStatus(StatusReport),
    Mail(MailCount),
    AskPending(PendingAsk),
    AskSettled {
        ask: AskId,
        settlement: AskSettlement,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Runtime,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub scope: Scope,
    pub seq: Seq,
    #[serde(default)]
    pub bus: BusSnapshot,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "feed", rename_all = "snake_case")]
pub enum Feed {
    Snapshot(Snapshot),
    Event(Event),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_event_round_trips_through_json() -> serde_json::Result<()> {
        let event = Event {
            seq: Seq(4),
            kind: EventKind::Pong {
                request: RequestId(9),
            },
        };
        let json = serde_json::to_string(&event)?;
        assert_eq!(json, r#"{"seq":4,"kind":{"type":"pong","request":9}}"#);
        assert_eq!(serde_json::from_str::<Event>(&json)?, event);
        Ok(())
    }

    #[test]
    fn an_event_from_a_newer_runtime_reads_as_unknown_with_its_seq() -> serde_json::Result<()> {
        let event: Event =
            serde_json::from_str(r#"{"seq":7,"kind":{"type":"session_opened","session":3}}"#)?;
        assert_eq!(
            event,
            Event {
                seq: Seq(7),
                kind: EventKind::Unknown,
            }
        );
        Ok(())
    }

    #[test]
    fn a_feed_tells_a_snapshot_from_an_event() -> serde_json::Result<()> {
        let feeds = [
            Feed::Snapshot(Snapshot {
                scope: Scope::Runtime,
                seq: Seq(2),
                bus: BusSnapshot::default(),
            }),
            Feed::Event(Event {
                seq: Seq(3),
                kind: EventKind::Pong {
                    request: RequestId(1),
                },
            }),
        ];
        for feed in feeds {
            let json = serde_json::to_string(&feed)?;
            assert_eq!(serde_json::from_str::<Feed>(&json)?, feed);
        }
        Ok(())
    }
}
