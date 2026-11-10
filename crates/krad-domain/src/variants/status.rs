use super::run::VariantWorktree;
use crate::ids::TerminalId;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{Duration, SystemTime};

pub const MAX_TEST_TAIL_BYTES: usize = 8 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestVerdict {
    Passed,
    Failed,
    CouldNotRun,
}

impl TestVerdict {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Passed => "Tests passed",
            Self::Failed => "Tests failed",
            Self::CouldNotRun => "Tests could not run",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestOutcome {
    pub verdict: TestVerdict,
    pub took: Duration,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tail: String,
}

impl TestOutcome {
    #[must_use]
    pub fn new(verdict: TestVerdict, took: Duration, output: &str) -> Self {
        Self {
            verdict,
            took,
            tail: output_tail(output, MAX_TEST_TAIL_BYTES).to_owned(),
        }
    }
}

fn output_tail(output: &str, limit: usize) -> &str {
    if output.len() <= limit {
        return output;
    }
    let mut start = output.len() - limit;
    while !output.is_char_boundary(start) {
        start += 1;
    }
    &output[start..]
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum VariantStatus {
    Launching,
    Working,
    Finished,
    Testing,
    Tested { verdict: TestVerdict },
    Failed { reason: String },
    Missing,
    Discarded,
    Chosen,
}

impl VariantStatus {
    #[must_use]
    pub const fn is_settled(&self) -> bool {
        !matches!(self, Self::Launching | Self::Working | Self::Testing)
    }

    #[must_use]
    pub const fn is_closed(&self) -> bool {
        matches!(self, Self::Discarded | Self::Chosen)
    }

    #[must_use]
    pub const fn can_win(&self) -> bool {
        matches!(self, Self::Finished | Self::Tested { .. })
    }

    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Launching => "Starting",
            Self::Working => "Working",
            Self::Finished => "Finished",
            Self::Testing => "Running tests",
            Self::Tested { verdict } => verdict.label(),
            Self::Failed { .. } => "Failed",
            Self::Missing => "Worktree missing",
            Self::Discarded => "Discarded",
            Self::Chosen => "Winner",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VariantEvent {
    Launched {
        worktree: VariantWorktree,
        terminal: TerminalId,
        at: SystemTime,
    },
    LaunchFailed(String),
    Working,
    Finished {
        at: SystemTime,
    },
    TestsStarted,
    TestsDone(TestOutcome),
    WorktreeMissing,
    Discarded,
    Chosen,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransitionError {
    pub from: VariantStatus,
    pub event: &'static str,
}

impl fmt::Display for TransitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "a variant that is {} cannot take {}",
            self.from.label(),
            self.event
        )
    }
}

impl std::error::Error for TransitionError {}

impl VariantEvent {
    const fn name(&self) -> &'static str {
        match self {
            Self::Launched { .. } => "a launch",
            Self::LaunchFailed(_) => "a failed launch",
            Self::Working => "new work",
            Self::Finished { .. } => "an end of work",
            Self::TestsStarted => "a test run",
            Self::TestsDone(_) => "a test result",
            Self::WorktreeMissing => "a missing worktree",
            Self::Discarded => "a discard",
            Self::Chosen => "a win",
        }
    }
}

impl VariantStatus {
    /// # Errors
    ///
    /// Returns a [`TransitionError`] when the event does not apply to a
    /// variant in this status, as a test result for a variant that went back
    /// to work or any event for a discarded or chosen variant.
    pub fn next(&self, event: &VariantEvent) -> Result<Self, TransitionError> {
        use VariantEvent as Event;
        let next = match (self, event) {
            (Self::Launching, Event::LaunchFailed(reason)) => Some(Self::Failed {
                reason: reason.clone(),
            }),
            (Self::Launching, Event::Launched { .. })
            | (
                Self::Working | Self::Finished | Self::Testing | Self::Tested { .. },
                Event::Working,
            ) => Some(Self::Working),
            (Self::Working, Event::Finished { .. }) => Some(Self::Finished),
            (Self::Finished | Self::Tested { .. }, Event::TestsStarted) => Some(Self::Testing),
            (Self::Testing, Event::TestsDone(outcome)) => Some(Self::Tested {
                verdict: outcome.verdict,
            }),
            (status, Event::WorktreeMissing) if !status.is_closed() => Some(Self::Missing),
            (status, Event::Discarded) if !status.is_closed() => Some(Self::Discarded),
            (status, Event::Chosen) if status.can_win() => Some(Self::Chosen),
            _ => None,
        };
        next.ok_or_else(|| TransitionError {
            from: self.clone(),
            event: event.name(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::UNIX_EPOCH;

    fn launched() -> VariantEvent {
        VariantEvent::Launched {
            worktree: VariantWorktree {
                path: PathBuf::from("/repo.worktrees/variant-1-1"),
                branch: "variant-1-1".to_owned(),
                group: crate::ids::GroupId(3),
            },
            terminal: TerminalId(9),
            at: UNIX_EPOCH,
        }
    }

    fn outcome(verdict: TestVerdict) -> VariantEvent {
        VariantEvent::TestsDone(TestOutcome::new(verdict, Duration::from_secs(3), "ok"))
    }

    fn walk(events: &[VariantEvent]) -> Result<VariantStatus, TransitionError> {
        events
            .iter()
            .try_fold(VariantStatus::Launching, |status, event| status.next(event))
    }

    #[test]
    fn a_variant_launches_works_finishes_and_is_tested() {
        let finished = VariantEvent::Finished { at: UNIX_EPOCH };
        assert_eq!(
            walk(&[launched(), finished.clone()]),
            Ok(VariantStatus::Finished)
        );
        assert_eq!(
            walk(&[
                launched(),
                finished,
                VariantEvent::TestsStarted,
                outcome(TestVerdict::Failed)
            ]),
            Ok(VariantStatus::Tested {
                verdict: TestVerdict::Failed
            })
        );
    }

    #[test]
    fn an_agent_that_works_again_drops_back_to_working() {
        let finished = VariantEvent::Finished { at: UNIX_EPOCH };
        let events = [
            launched(),
            finished,
            VariantEvent::TestsStarted,
            outcome(TestVerdict::Passed),
            VariantEvent::Working,
        ];
        assert_eq!(walk(&events), Ok(VariantStatus::Working));
        let stale = walk(&[launched(), outcome(TestVerdict::Passed)]);
        assert!(stale.is_err());
    }

    #[test]
    fn only_a_finished_variant_can_win_any_open_one_can_be_discarded() {
        assert!(walk(&[launched(), VariantEvent::Chosen]).is_err());
        assert_eq!(
            walk(&[launched(), VariantEvent::Discarded]),
            Ok(VariantStatus::Discarded)
        );
        let failed = walk(&[VariantEvent::LaunchFailed("no claude".to_owned())]);
        assert_eq!(
            failed,
            Ok(VariantStatus::Failed {
                reason: "no claude".to_owned()
            })
        );
        let discarded = failed.and_then(|status| status.next(&VariantEvent::Discarded));
        assert_eq!(discarded, Ok(VariantStatus::Discarded));
        let after = discarded.and_then(|status| status.next(&VariantEvent::WorktreeMissing));
        assert!(after.is_err());
    }

    #[test]
    fn a_missing_worktree_settles_a_variant_that_can_then_be_discarded() {
        let missing = walk(&[launched(), VariantEvent::WorktreeMissing]);
        assert_eq!(missing, Ok(VariantStatus::Missing));
        assert!(missing.as_ref().is_ok_and(VariantStatus::is_settled));
        assert!(missing.is_ok_and(|status| !status.can_win()));
    }

    #[test]
    fn the_test_output_keeps_only_its_tail_on_a_character_boundary() {
        let long = format!("{}é{}", "a".repeat(MAX_TEST_TAIL_BYTES), "b".repeat(10));
        let kept = TestOutcome::new(TestVerdict::Passed, Duration::ZERO, &long);
        assert!(kept.tail.len() <= MAX_TEST_TAIL_BYTES);
        assert!(kept.tail.ends_with("bbbbbbbbbb"));
    }
}
