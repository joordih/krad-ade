use krad_domain::{
    AgentStatus, ApiDiscoveryFile, EnvironmentName, TerminalId, ToolDecision, ToolUseId,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct AgentRef {
    pub environment: EnvironmentName,
    pub terminal: TerminalId,
}

#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default, Serialize, Deserialize,
)]
pub struct AskId(pub u64);

impl AskId {
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct AskRequest {
    pub tool: String,
    pub input: Value,
    #[serde(default)]
    pub suggestions: Vec<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call: Option<ToolUseId>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct PendingAsk {
    pub ask: AskId,
    pub agent: AgentRef,
    pub request: AskRequest,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AskSettlement {
    Answered,
    CallerLeft,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct StatusReport {
    pub agent: AgentRef,
    pub status: AgentStatus,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct MailCount {
    pub agent: AgentRef,
    pub unread: usize,
}

#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct BusSnapshot {
    #[serde(default)]
    pub statuses: Vec<StatusReport>,
    #[serde(default)]
    pub mail: Vec<MailCount>,
    #[serde(default)]
    pub asks: Vec<PendingAsk>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum BusCommand {
    AnswerAsk {
        ask: AskId,
        decision: ToolDecision,
    },
    PublishDiscovery {
        file: ApiDiscoveryFile,
    },
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{Command, ErrorCode, Reply};
    use crate::event::{Event, EventKind, Scope, Snapshot};
    use crate::ids::Seq;

    fn agent() -> AgentRef {
        AgentRef {
            environment: EnvironmentName::new("main".to_owned()),
            terminal: TerminalId(3),
        }
    }

    #[test]
    fn answering_an_ask_names_the_ask_and_the_decision() -> serde_json::Result<()> {
        let command = Command::Bus(BusCommand::AnswerAsk {
            ask: AskId(7),
            decision: ToolDecision::Pass,
        });
        let json = serde_json::to_string(&command)?;
        assert_eq!(
            json,
            r#"{"area":"bus","verb":"answer_ask","ask":7,"decision":"Pass"}"#
        );
        assert_eq!(serde_json::from_str::<Command>(&json)?, command);
        let newer: Command = serde_json::from_str(r#"{"area":"bus","verb":"forget_ask"}"#)?;
        assert_eq!(newer, Command::Bus(BusCommand::Unknown));
        Ok(())
    }

    #[test]
    fn a_second_answer_is_refused_with_a_stable_code() -> serde_json::Result<()> {
        let reply = Reply::Refused {
            code: ErrorCode::AlreadyAnswered,
            message: "answered".to_owned(),
        };
        let json = serde_json::to_string(&reply)?;
        assert!(json.contains(r#""code":"already_answered""#));
        assert_eq!(serde_json::from_str::<Reply>(&json)?, reply);
        Ok(())
    }

    #[test]
    fn bus_events_round_trip_through_json() -> serde_json::Result<()> {
        let events = [
            EventKind::AskPending(PendingAsk {
                ask: AskId(1),
                agent: agent(),
                request: AskRequest {
                    tool: "Bash".to_owned(),
                    input: serde_json::json!({ "command": "ls" }),
                    suggestions: Vec::new(),
                    call: Some(ToolUseId::from("toolu_1")),
                },
            }),
            EventKind::AskSettled {
                ask: AskId(1),
                settlement: AskSettlement::CallerLeft,
            },
            EventKind::AgentStatus(StatusReport {
                agent: agent(),
                status: AgentStatus {
                    context_used: Some(12),
                    ..AgentStatus::default()
                },
            }),
            EventKind::Mail(MailCount {
                agent: agent(),
                unread: 2,
            }),
        ];
        for (seq, kind) in (1..).zip(events) {
            let event = Event {
                seq: Seq(seq),
                kind,
            };
            let json = serde_json::to_string(&event)?;
            assert_eq!(serde_json::from_str::<Event>(&json)?, event);
        }
        Ok(())
    }

    #[test]
    fn a_snapshot_without_bus_state_reads_as_an_empty_bus() -> serde_json::Result<()> {
        let snapshot: Snapshot = serde_json::from_str(r#"{"scope":"runtime","seq":4}"#)?;
        assert_eq!(snapshot.seq, Seq(4));
        assert_eq!(snapshot.scope, Scope::Runtime);
        assert_eq!(snapshot.bus, BusSnapshot::default());
        Ok(())
    }
}
