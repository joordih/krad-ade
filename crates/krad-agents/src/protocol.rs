use crate::handle::Caller;
use krad_domain::{ToolDecision, ASK_WAIT_SECS};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
use std::time::Duration;

pub const MAX_WAIT_SECS: u64 = 1800;
const BLOCKING_MARGIN: Duration = Duration::from_secs(1);
const DEFAULT_DEADLINE: Duration = Duration::from_secs(10);
const SPAWN_DEADLINE: Duration = Duration::from_mins(1);
const NOT_THE_LEAD: &str =
    "Only the session's lead can start or direct workers: ask the user in the lead chat";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentStatus {
    pub model_id: Option<String>,
    pub model_name: Option<String>,
    pub context_used: Option<u16>,
    pub effort: Option<String>,
    pub thinking: Option<bool>,
    pub five_hour_used: Option<u16>,
    pub seven_day_used: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Request {
    List,
    Send {
        to: u64,
        body: String,
    },
    Check {
        wait_secs: Option<u64>,
    },
    Wait {
        target: u64,
        timeout_secs: u64,
    },
    Read {
        target: u64,
        lines: usize,
    },
    Hook {
        harness: String,
        event: String,
        tool: Option<String>,
        detail: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resume: Option<String>,
    },
    Status(AgentStatus),
    Ask {
        tool: String,
        input: Value,
        #[serde(default)]
        suggestions: Vec<Value>,
    },
    Spawn {
        agent: String,
        worktree: Option<String>,
        task: String,
    },
}

impl Request {
    #[must_use]
    pub fn deadline(&self) -> Duration {
        match self {
            Self::Check {
                wait_secs: Some(secs),
            }
            | Self::Wait {
                timeout_secs: secs, ..
            } => Duration::from_secs((*secs).min(MAX_WAIT_SECS)) + BLOCKING_MARGIN,
            Self::Ask { .. } => Duration::from_secs(ASK_WAIT_SECS) + BLOCKING_MARGIN,
            Self::List
            | Self::Send { .. }
            | Self::Check { wait_secs: None }
            | Self::Read { .. }
            | Self::Hook { .. }
            | Self::Status(_) => DEFAULT_DEADLINE,
            Self::Spawn { .. } => SPAWN_DEADLINE,
        }
    }

    #[must_use]
    pub const fn directs_workers(&self) -> bool {
        matches!(self, Self::Spawn { .. } | Self::Send { .. })
    }
}

#[must_use]
pub fn lead_refusal(from: &Caller, request: &Request) -> Option<Response> {
    let is_lead = matches!(from, Caller::Lead(_));
    (!is_lead && request.directs_workers()).then(|| Response::Error(NOT_THE_LEAD.to_owned()))
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BusToken(String);

impl BusToken {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for BusToken {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for BusToken {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl fmt::Debug for BusToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    pub token: BusToken,
    pub from: Caller,
    pub request: Request,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentInfo {
    pub terminal: u64,
    pub title: String,
    pub group: Option<String>,
    pub state: String,
    pub harness: Option<String>,
    pub is_self: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub id: u64,
    pub from: u64,
    pub from_title: String,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Response {
    Agents(Vec<AgentInfo>),
    Sent { id: u64 },
    Messages(Vec<Message>),
    Settled { target: u64, state: String },
    TimedOut,
    Screen(String),
    Noted,
    Spawned { terminal: u64 },
    Submitted { terminal: u64 },
    Decided(ToolDecision),
    Error(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handle::{ChatHandle, LeadHandle, TerminalHandle};

    #[test]
    fn envelope_roundtrips_through_json() -> Result<(), serde_json::Error> {
        let envelope = Envelope {
            token: BusToken::from("secret"),
            from: Caller::Terminal(TerminalHandle {
                environment: "main".to_owned(),
                terminal: 3,
            }),
            request: Request::Send {
                to: 4,
                body: "done: see PR".to_owned(),
            },
        };
        let line = serde_json::to_string(&envelope)?;
        assert!(!line.contains('\n'));
        assert_eq!(serde_json::from_str::<Envelope>(&line)?, envelope);
        Ok(())
    }

    #[test]
    fn only_a_lead_may_start_or_direct_workers() {
        let spawn = Request::Spawn {
            agent: "codex".to_owned(),
            worktree: None,
            task: "tests".to_owned(),
        };
        let send = Request::Send {
            to: 4,
            body: "also cover clock skew".to_owned(),
        };
        let worker = Caller::Terminal(TerminalHandle {
            environment: "main".to_owned(),
            terminal: 4,
        });
        let side_chat = Caller::Chat(ChatHandle {
            environment: "main".to_owned(),
            group: 1,
        });
        let lead = Caller::Lead(LeadHandle {
            environment: "main".to_owned(),
            session: 2,
        });
        for request in [&spawn, &send] {
            assert!(matches!(
                lead_refusal(&worker, request),
                Some(Response::Error(_))
            ));
            assert!(lead_refusal(&side_chat, request).is_some());
            assert_eq!(lead_refusal(&lead, request), None);
        }
        assert_eq!(lead_refusal(&worker, &Request::List), None);
    }

    #[test]
    fn debug_hides_the_token() {
        let envelope = Envelope {
            token: BusToken::from("secret"),
            from: Caller::Terminal(TerminalHandle {
                environment: "main".to_owned(),
                terminal: 3,
            }),
            request: Request::Send {
                to: 4,
                body: "hi".to_owned(),
            },
        };
        let printed = format!("{envelope:?}");
        assert!(!printed.contains("secret"));
        assert!(printed.contains("[REDACTED]"));
    }
}
