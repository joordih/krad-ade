use super::limiter::RunRefusal;
use super::trigger::KraddieWake;
use crate::ids::{EnvironmentName, ResumeId, SessionId, TerminalId};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::SystemTime;

pub const MAX_KRADDIE_HISTORY: usize = 50;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct KraddieRunId(pub u64);

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KraddieSessionLink {
    pub environment: EnvironmentName,
    pub session: SessionId,
    pub terminal: TerminalId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum KraddieOutcome {
    Starting,
    Working,
    NeedsYou,
    Finished,
    Failed { reason: String },
    Refused { refusal: RunRefusal },
    Interrupted,
}

impl KraddieOutcome {
    #[must_use]
    pub const fn is_live(&self) -> bool {
        matches!(self, Self::Starting | Self::Working | Self::NeedsYou)
    }

    #[must_use]
    pub fn label(&self) -> &str {
        match self {
            Self::Starting => "Starting",
            Self::Working => "Working",
            Self::NeedsYou => "Needs you",
            Self::Finished => "Finished",
            Self::Failed { reason } => reason,
            Self::Refused { refusal } => match refusal {
                RunRefusal::AlreadyRunning => "Skipped: already running",
                RunRefusal::AtCapacity => "Skipped: too many runs at once",
            },
            Self::Interrupted => "Interrupted when the app closed",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KraddieRun {
    pub id: KraddieRunId,
    pub started: SystemTime,
    pub wake: KraddieWake,
    pub outcome: KraddieOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<KraddieSessionLink>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume: Option<ResumeId>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KraddieHistory {
    runs: VecDeque<KraddieRun>,
}

impl KraddieHistory {
    pub fn start(
        &mut self,
        started: SystemTime,
        wake: KraddieWake,
        outcome: KraddieOutcome,
    ) -> KraddieRunId {
        let id = KraddieRunId(
            self.runs
                .back()
                .map_or(1, |last| last.id.0.saturating_add(1)),
        );
        self.runs.push_back(KraddieRun {
            id,
            started,
            wake,
            outcome,
            session: None,
            resume: None,
        });
        while self.runs.len() > MAX_KRADDIE_HISTORY {
            self.runs.pop_front();
        }
        id
    }

    pub fn run_mut(&mut self, id: KraddieRunId) -> Option<&mut KraddieRun> {
        self.runs.iter_mut().find(|run| run.id == id)
    }

    pub fn settle(&mut self, id: KraddieRunId, outcome: KraddieOutcome) -> bool {
        let Some(run) = self.run_mut(id) else {
            return false;
        };
        if run.outcome == outcome {
            return false;
        }
        run.outcome = outcome;
        true
    }

    pub fn forget(&mut self, id: KraddieRunId) -> bool {
        let before = self.runs.len();
        self.runs.retain(|run| run.id != id);
        self.runs.len() != before
    }

    pub fn interrupt_live(&mut self) -> bool {
        let mut changed = false;
        for run in self.runs.iter_mut().filter(|run| run.outcome.is_live()) {
            run.outcome = KraddieOutcome::Interrupted;
            changed = true;
        }
        changed
    }

    #[must_use]
    pub fn last_started(&self) -> Option<SystemTime> {
        self.runs
            .iter()
            .filter(|run| !matches!(run.outcome, KraddieOutcome::Refused { .. }))
            .map(|run| run.started)
            .max()
    }

    #[must_use]
    pub fn last_resume(&self) -> Option<&ResumeId> {
        self.newest_first().find_map(|run| run.resume.as_ref())
    }

    #[must_use]
    pub fn finished(&self) -> usize {
        self.runs
            .iter()
            .filter(|run| matches!(run.outcome, KraddieOutcome::Finished))
            .count()
    }

    #[must_use]
    pub fn latest(&self) -> Option<&KraddieRun> {
        self.runs.back()
    }

    pub fn newest_first(&self) -> impl Iterator<Item = &KraddieRun> {
        self.runs.iter().rev()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.runs.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn history_keeps_the_latest_runs_and_numbers_them() {
        let mut history = KraddieHistory::default();
        let at = SystemTime::UNIX_EPOCH;
        let first = history.start(at, KraddieWake::Manual, KraddieOutcome::Starting);
        assert_eq!(first, KraddieRunId(1));
        for step in 0..MAX_KRADDIE_HISTORY {
            let later = at
                + Duration::from_secs(u64::try_from(step).unwrap_or_else(|_| unreachable!()) + 1);
            history.start(later, KraddieWake::Schedule, KraddieOutcome::Finished);
        }
        assert_eq!(history.len(), MAX_KRADDIE_HISTORY);
        assert!(history.run_mut(first).is_none());
        assert_eq!(
            history.latest().map(|run| run.id),
            Some(KraddieRunId(
                u64::try_from(MAX_KRADDIE_HISTORY).unwrap_or_else(|_| unreachable!()) + 1
            ))
        );
    }

    #[test]
    fn live_runs_become_interrupted_and_refusals_do_not_count_as_runs() {
        let mut history = KraddieHistory::default();
        let at = SystemTime::UNIX_EPOCH + Duration::from_secs(10);
        let run = history.start(at, KraddieWake::Cli, KraddieOutcome::Working);
        let refused = KraddieOutcome::Refused {
            refusal: RunRefusal::AtCapacity,
        };
        history.start(at + Duration::from_secs(5), KraddieWake::Schedule, refused);
        assert_eq!(history.last_started(), Some(at));
        assert!(history.settle(run, KraddieOutcome::NeedsYou));
        assert!(!history.settle(run, KraddieOutcome::NeedsYou));
        assert!(history.interrupt_live());
        assert_eq!(
            history.run_mut(run).map(|run| run.outcome.clone()),
            Some(KraddieOutcome::Interrupted)
        );
        assert!(!history.interrupt_live());
        assert_eq!(history.last_resume(), None);
        let resume = ResumeId::try_from("abc-1".to_owned()).unwrap_or_else(|_| unreachable!());
        if let Some(run) = history.run_mut(run) {
            run.resume = Some(resume.clone());
        }
        assert_eq!(history.last_resume(), Some(&resume));
        assert_eq!(history.finished(), 0);
        assert!(history.forget(run));
        assert!(!history.forget(run));
        assert_eq!(history.len(), 1);
        let line = serde_json::to_string(&history).unwrap_or_else(|_| unreachable!());
        assert_eq!(
            serde_json::from_str::<KraddieHistory>(&line).unwrap_or_else(|_| unreachable!()),
            history
        );
    }
}
