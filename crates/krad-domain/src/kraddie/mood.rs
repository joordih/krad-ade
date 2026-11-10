use super::history::KraddieOutcome;
use crate::agent::AgentState;
use serde::{Deserialize, Serialize};

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum KraddieMood {
    #[default]
    Idle,
    Working,
    Waiting,
    Done,
    Error,
}

impl KraddieMood {
    pub const ALL: [Self; 5] = [
        Self::Idle,
        Self::Working,
        Self::Waiting,
        Self::Done,
        Self::Error,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Working => "Working",
            Self::Waiting => "Waiting",
            Self::Done => "Done",
            Self::Error => "Error",
        }
    }

    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Idle => 0,
            Self::Working => 1,
            Self::Waiting => 2,
            Self::Done => 3,
            Self::Error => 4,
        }
    }

    #[must_use]
    pub const fn of_agent(state: AgentState) -> Self {
        match state {
            AgentState::Idle => Self::Idle,
            AgentState::Working => Self::Working,
            AgentState::WaitingForInput => Self::Waiting,
            AgentState::Done => Self::Done,
        }
    }

    #[must_use]
    pub const fn of_outcome(outcome: &KraddieOutcome) -> Self {
        match outcome {
            KraddieOutcome::Starting | KraddieOutcome::Working => Self::Working,
            KraddieOutcome::NeedsYou => Self::Waiting,
            KraddieOutcome::Finished => Self::Done,
            KraddieOutcome::Failed { .. } | KraddieOutcome::Interrupted => Self::Error,
            KraddieOutcome::Refused { .. } => Self::Idle,
        }
    }

    #[must_use]
    pub fn of_run(agent: Option<AgentState>, outcome: Option<&KraddieOutcome>) -> Self {
        match (outcome, agent) {
            (Some(outcome @ (KraddieOutcome::Failed { .. } | KraddieOutcome::Interrupted)), _) => {
                Self::of_outcome(outcome)
            }
            (_, Some(state)) => Self::of_agent(state),
            (Some(outcome), None) => Self::of_outcome(outcome),
            (None, None) => Self::Idle,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_agent_state_shows_its_own_mood() {
        assert_eq!(KraddieMood::of_agent(AgentState::Idle), KraddieMood::Idle);
        assert_eq!(
            KraddieMood::of_agent(AgentState::Working),
            KraddieMood::Working
        );
        assert_eq!(
            KraddieMood::of_agent(AgentState::WaitingForInput),
            KraddieMood::Waiting
        );
        assert_eq!(KraddieMood::of_agent(AgentState::Done), KraddieMood::Done);
        let moods: Vec<usize> = KraddieMood::ALL.iter().map(|mood| mood.index()).collect();
        assert_eq!(moods, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn a_failed_run_looks_like_an_error_whatever_its_agent_says() {
        let failed = KraddieOutcome::Failed {
            reason: "no repo".to_owned(),
        };
        assert_eq!(
            KraddieMood::of_run(Some(AgentState::Idle), Some(&failed)),
            KraddieMood::Error
        );
        assert_eq!(
            KraddieMood::of_run(
                Some(AgentState::WaitingForInput),
                Some(&KraddieOutcome::Working)
            ),
            KraddieMood::Waiting
        );
        assert_eq!(
            KraddieMood::of_run(None, Some(&KraddieOutcome::Finished)),
            KraddieMood::Done
        );
        assert_eq!(KraddieMood::of_run(None, None), KraddieMood::Idle);
    }
}
