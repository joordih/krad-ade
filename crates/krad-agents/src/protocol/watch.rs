use super::harness_name;
use super::{AgentInfo, TerminalState, Timestamp};
use krad_domain::{AgentPreset, AgentState};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StreamId(pub String);

impl fmt::Display for StreamId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StateReason {
    TurnStarted,
    TurnFinished,
    AskPending,
    PermissionPending,
    InputPending,
    ProcessExited,
    UserClosed,
    EnvironmentClosed,
}

impl StateReason {
    pub const ALL: [Self; 8] = [
        Self::TurnStarted,
        Self::TurnFinished,
        Self::AskPending,
        Self::PermissionPending,
        Self::InputPending,
        Self::ProcessExited,
        Self::UserClosed,
        Self::EnvironmentClosed,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::TurnStarted => "turn_started",
            Self::TurnFinished => "turn_finished",
            Self::AskPending => "ask_pending",
            Self::PermissionPending => "permission_pending",
            Self::InputPending => "input_pending",
            Self::ProcessExited => "process_exited",
            Self::UserClosed => "user_closed",
            Self::EnvironmentClosed => "environment_closed",
        }
    }

    #[must_use]
    pub const fn inferred(previous: Option<TerminalState>, state: TerminalState) -> Option<Self> {
        match (previous, state) {
            (None, _) | (_, TerminalState::Closed) => None,
            (_, TerminalState::Exited) => Some(Self::ProcessExited),
            (_, TerminalState::Running(AgentState::Working)) => Some(Self::TurnStarted),
            (_, TerminalState::Running(AgentState::WaitingForInput)) => Some(Self::InputPending),
            (
                Some(TerminalState::Running(AgentState::Working)),
                TerminalState::Running(AgentState::Done | AgentState::Idle),
            ) => Some(Self::TurnFinished),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentChangeKind {
    Opened,
    StateChanged,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
pub struct AgentEvent {
    pub seq: u64,
    pub at: Timestamp,
    pub kind: AgentChangeKind,
    pub environment: String,
    pub terminal: u64,
    pub title: String,
    pub group: Option<String>,
    #[serde(with = "harness_name::display_name")]
    pub harness: Option<AgentPreset>,
    pub previous: Option<TerminalState>,
    pub state: TerminalState,
    pub coarse_state: CoarseState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<StateReason>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSnapshot {
    pub stream: StreamId,
    pub seq: u64,
    pub at: Timestamp,
    pub resync: bool,
    pub agents: Vec<AgentInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Heartbeat {
    pub stream: StreamId,
    pub seq: u64,
    pub at: Timestamp,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentFilter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
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
pub struct AgentSubscription {
    #[serde(default)]
    pub filter: AgentFilter,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since_seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<StreamId>,
}
