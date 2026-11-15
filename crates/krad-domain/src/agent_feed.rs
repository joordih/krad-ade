use serde::{Deserialize, Serialize};
use std::fmt;

use crate::{AgentPreset, AgentState, UnknownAgentState};

pub mod harness_names;

const EXITED: &str = "exited";
const CLOSED: &str = "closed";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AgentInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    pub terminal: u64,
    pub title: String,
    pub group: Option<String>,
    pub state: TerminalState,
    #[cfg_attr(feature = "ts", ts(as = "Option<String>"))]
    #[serde(with = "harness_names::display_name")]
    pub harness: Option<AgentPreset>,
    pub is_self: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(as = "String"))]
#[serde(into = "String", try_from = "String")]
pub enum TerminalState {
    Running(AgentState),
    Exited,
    Closed,
}

impl fmt::Display for TerminalState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Running(state) => state.label(),
            Self::Exited => EXITED,
            Self::Closed => CLOSED,
        })
    }
}

impl From<TerminalState> for String {
    fn from(state: TerminalState) -> Self {
        state.to_string()
    }
}

impl TryFrom<String> for TerminalState {
    type Error = UnknownAgentState;

    fn try_from(label: String) -> Result<Self, Self::Error> {
        match label.as_str() {
            EXITED => return Ok(Self::Exited),
            CLOSED => return Ok(Self::Closed),
            _ => {}
        }
        label.parse().map(Self::Running)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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
