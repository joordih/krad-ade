use super::harness_name;
use super::{AgentInfo, StateReason, TerminalState, Timestamp};
use crate::{AgentPreset, AgentState};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(transparent)]
pub struct StreamId(pub String);

impl fmt::Display for StreamId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum AgentChangeKind {
    Opened,
    StateChanged,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "kebab-case")]
pub enum CoarseState {
    Idle,
    Working,
    WaitingForInput,
    Closed,
}

impl From<TerminalState> for CoarseState {
    fn from(state: TerminalState) -> Self {
        match state {
            TerminalState::Running(AgentState::Idle | AgentState::Done) => Self::Idle,
            TerminalState::Running(AgentState::Working) => Self::Working,
            TerminalState::Running(AgentState::WaitingForInput) => Self::WaitingForInput,
            TerminalState::Exited | TerminalState::Closed => Self::Closed,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AgentEvent {
    pub seq: u64,
    pub at: Timestamp,
    pub kind: AgentChangeKind,
    pub environment: String,
    pub terminal: u64,
    pub title: String,
    pub group: Option<String>,
    #[cfg_attr(feature = "ts", ts(as = "Option<String>"))]
    #[serde(with = "harness_name::display_name")]
    pub harness: Option<AgentPreset>,
    pub previous: Option<TerminalState>,
    pub state: TerminalState,
    pub coarse_state: CoarseState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<StateReason>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AgentSnapshot {
    pub stream: StreamId,
    pub seq: u64,
    pub at: Timestamp,
    pub resync: bool,
    pub agents: Vec<AgentInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Heartbeat {
    pub stream: StreamId,
    pub seq: u64,
    pub at: Timestamp,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AgentFilter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[cfg_attr(feature = "ts", ts(as = "Option<String>"))]
    #[serde(
        default,
        with = "harness_name::display_name",
        skip_serializing_if = "Option::is_none"
    )]
    pub harness: Option<AgentPreset>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub terminals: Vec<u64>,
}

impl AgentFilter {
    #[must_use]
    pub fn admits_agent(&self, agent: &AgentInfo) -> bool {
        self.admits(&Seen {
            environment: agent.environment.as_deref(),
            terminal: agent.terminal,
            group: agent.group.as_deref(),
            harness: agent.harness,
        })
    }

    #[must_use]
    pub fn admits_event(&self, event: &AgentEvent) -> bool {
        self.admits(&Seen {
            environment: Some(&event.environment),
            terminal: event.terminal,
            group: event.group.as_deref(),
            harness: event.harness,
        })
    }

    fn admits(&self, seen: &Seen<'_>) -> bool {
        self.environment
            .as_deref()
            .is_none_or(|wanted| seen.environment == Some(wanted))
            && self
                .group
                .as_deref()
                .is_none_or(|wanted| seen.group == Some(wanted))
            && self
                .harness
                .is_none_or(|wanted| seen.harness == Some(wanted))
            && (self.terminals.is_empty() || self.terminals.contains(&seen.terminal))
    }
}

struct Seen<'a> {
    environment: Option<&'a str>,
    terminal: u64,
    group: Option<&'a str>,
    harness: Option<AgentPreset>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AgentSubscription {
    #[serde(default)]
    pub filter: AgentFilter,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since_seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<StreamId>,
}
