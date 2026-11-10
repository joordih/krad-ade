use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkingCopy {
    ThisRepo,
    #[default]
    FreshWorktree,
}

impl WorkingCopy {
    pub const ALL: [Self; 2] = [Self::ThisRepo, Self::FreshWorktree];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::ThisRepo => "Right here",
            Self::FreshWorktree => "A worktree of its own",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Conversation {
    #[default]
    StartFresh,
    ContinueLast,
}

impl Conversation {
    pub const ALL: [Self; 2] = [Self::StartFresh, Self::ContinueLast];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::StartFresh => "Begin anew",
            Self::ContinueLast => "Pick up where it stopped",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AutomationSession {
    #[serde(default)]
    pub working_copy: WorkingCopy,
    #[serde(default)]
    pub conversation: Conversation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatchUp {
    #[default]
    Off,
    QuarterHour,
    Hour,
    SixHours,
    Day,
}

impl CatchUp {
    pub const ALL: [Self; 5] = [
        Self::Off,
        Self::QuarterHour,
        Self::Hour,
        Self::SixHours,
        Self::Day,
    ];

    #[must_use]
    pub const fn window(self) -> Option<Duration> {
        match self {
            Self::Off => None,
            Self::QuarterHour => Some(Duration::from_mins(15)),
            Self::Hour => Some(Duration::from_hours(1)),
            Self::SixHours => Some(Duration::from_hours(6)),
            Self::Day => Some(Duration::from_hours(24)),
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "Let it go",
            Self::QuarterHour => "Within 15 min",
            Self::Hour => "Within an hour",
            Self::SixHours => "Within 6 hours",
            Self::Day => "Within a day",
        }
    }
}
