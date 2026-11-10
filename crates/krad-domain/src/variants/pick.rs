use super::run::{RunDecision, VariantId, VariantRun, VariantRunId, VariantWorktree};
use super::status::VariantEvent;
use crate::ids::TerminalId;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WinnerOutcome {
    KeepWorktree,
    ApplyOntoBase,
}

impl WinnerOutcome {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::KeepWorktree => "Keep its worktree as the session",
            Self::ApplyOntoBase => "Apply its changes onto the base worktree",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplyStep {
    pub from: PathBuf,
    pub onto: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscardStep {
    pub variant: VariantId,
    pub terminal: Option<TerminalId>,
    pub worktree: Option<VariantWorktree>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CleanupPlan {
    pub run: VariantRunId,
    pub winner: VariantId,
    pub outcome: WinnerOutcome,
    pub apply: Option<ApplyStep>,
    pub discard: Vec<DiscardStep>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickError {
    AlreadyDecided,
    PickInProgress,
    UnknownVariant,
    NotFinished,
    NoWorktree,
}

impl fmt::Display for PickError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::AlreadyDecided => "This run already has a winner",
            Self::PickInProgress => "A winner of this run is already being picked",
            Self::UnknownVariant => "The run has no such variant",
            Self::NotFinished => "Only a variant that finished its work can win",
            Self::NoWorktree => "The winner's worktree is gone, so there is nothing to keep",
        })
    }
}

impl std::error::Error for PickError {}

impl VariantRun {
    /// # Errors
    ///
    /// Returns a [`PickError`] when the run is decided, the variant is not in
    /// it, has not finished or has lost its worktree.
    pub fn plan_pick(
        &self,
        winner: VariantId,
        outcome: WinnerOutcome,
    ) -> Result<CleanupPlan, PickError> {
        if self.decision.is_some() {
            return Err(PickError::AlreadyDecided);
        }
        if self.pending_pick.is_some() {
            return Err(PickError::PickInProgress);
        }
        let chosen = self.variant(winner).ok_or(PickError::UnknownVariant)?;
        if !chosen.status.can_win() {
            return Err(PickError::NotFinished);
        }
        let worktree = chosen.worktree.as_ref().ok_or(PickError::NoWorktree)?;
        let apply = (outcome == WinnerOutcome::ApplyOntoBase).then(|| ApplyStep {
            from: worktree.path.clone(),
            onto: self.base_root.clone(),
        });
        let discard = self
            .variants
            .iter()
            .filter(|variant| !variant.status.is_closed())
            .filter(|variant| variant.id != winner || apply.is_some())
            .map(|variant| DiscardStep {
                variant: variant.id,
                terminal: variant.terminal,
                worktree: variant.worktree.clone(),
            })
            .collect();
        Ok(CleanupPlan {
            run: self.id,
            winner,
            outcome,
            apply,
            discard,
        })
    }

    /// # Errors
    ///
    /// As [`VariantRun::plan_pick`]; on success the run holds the pick as
    /// pending until [`VariantRun::record_pick`] or
    /// [`VariantRun::abandon_pick`], and refuses any other pick meanwhile.
    pub fn begin_pick(
        &mut self,
        winner: VariantId,
        outcome: WinnerOutcome,
    ) -> Result<CleanupPlan, PickError> {
        let plan = self.plan_pick(winner, outcome)?;
        self.pending_pick = Some(winner);
        Ok(plan)
    }

    pub fn abandon_pick(&mut self) {
        self.pending_pick = None;
    }

    #[must_use]
    pub fn is_picking(&self) -> bool {
        self.pending_pick.is_some()
    }

    pub fn record_pick(&mut self, plan: &CleanupPlan) {
        self.pending_pick = None;
        let open = self
            .variants
            .iter_mut()
            .filter(|variant| !variant.status.is_closed());
        for variant in open {
            let event = if variant.id == plan.winner {
                VariantEvent::Chosen
            } else {
                VariantEvent::Discarded
            };
            variant.apply(event).unwrap_or_default();
        }
        self.decision = Some(RunDecision {
            winner: plan.winner,
            outcome: plan.outcome,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variants::fixtures::{finish, launch, run_of, worktree_of};
    use crate::variants::{TestVerdict, VariantStatus};
    use std::path::Path;
    use std::time::UNIX_EPOCH;

    fn three_finished() -> VariantRun {
        let mut run = run_of(VariantRunId(2), 3);
        for id in 1..=3 {
            launch(&mut run, VariantId(id), UNIX_EPOCH);
            finish(&mut run, VariantId(id), UNIX_EPOCH);
        }
        run
    }

    #[test]
    fn keeping_the_winner_discards_every_other_variant() -> Result<(), PickError> {
        let run = three_finished();
        let plan = run.plan_pick(VariantId(2), WinnerOutcome::KeepWorktree)?;
        assert_eq!(plan.apply, None);
        let discarded: Vec<VariantId> = plan.discard.iter().map(|step| step.variant).collect();
        assert_eq!(discarded, [VariantId(1), VariantId(3)]);
        assert_eq!(
            plan.discard[0].worktree,
            Some(worktree_of(&run, VariantId(1)))
        );
        Ok(())
    }

    #[test]
    fn applying_the_winner_onto_the_base_also_removes_its_worktree() -> Result<(), PickError> {
        let run = three_finished();
        let plan = run.plan_pick(VariantId(1), WinnerOutcome::ApplyOntoBase)?;
        let apply = plan.apply.clone();
        assert_eq!(
            apply.as_ref().map(|step| step.onto.as_path()),
            Some(Path::new("/repo"))
        );
        assert_eq!(
            apply.map(|step| step.from),
            Some(worktree_of(&run, VariantId(1)).path)
        );
        assert_eq!(plan.discard.len(), 3);
        Ok(())
    }

    #[test]
    fn a_variant_without_a_worktree_is_discarded_without_removing_anything() -> Result<(), PickError>
    {
        let mut run = three_finished();
        if let Some(variant) = run.variant_mut(VariantId(3)) {
            assert!(variant.apply(VariantEvent::WorktreeMissing).is_ok());
            variant.worktree = None;
        }
        let plan = run.plan_pick(VariantId(1), WinnerOutcome::KeepWorktree)?;
        assert_eq!(plan.discard[1].variant, VariantId(3));
        assert_eq!(plan.discard[1].worktree, None);
        Ok(())
    }

    #[test]
    fn a_pending_pick_refuses_a_second_pick_until_it_is_recorded_or_abandoned(
    ) -> Result<(), PickError> {
        let mut run = three_finished();
        let first = run.begin_pick(VariantId(1), WinnerOutcome::ApplyOntoBase)?;
        assert!(run.is_picking());
        assert_eq!(
            run.begin_pick(VariantId(1), WinnerOutcome::ApplyOntoBase),
            Err(PickError::PickInProgress)
        );
        assert_eq!(
            run.begin_pick(VariantId(2), WinnerOutcome::KeepWorktree),
            Err(PickError::PickInProgress)
        );
        run.abandon_pick();
        assert!(!run.is_picking());
        let again = run.begin_pick(VariantId(1), WinnerOutcome::ApplyOntoBase)?;
        assert_eq!(again, first);
        run.record_pick(&again);
        assert!(!run.is_picking());
        assert_eq!(
            run.begin_pick(VariantId(1), WinnerOutcome::ApplyOntoBase),
            Err(PickError::AlreadyDecided)
        );
        Ok(())
    }

    #[test]
    fn a_pending_pick_is_not_saved() -> Result<(), Box<dyn std::error::Error>> {
        let mut run = three_finished();
        run.begin_pick(VariantId(1), WinnerOutcome::ApplyOntoBase)?;
        let loaded: VariantRun = serde_json::from_str(&serde_json::to_string(&run)?)?;
        assert!(!loaded.is_picking());
        Ok(())
    }

    #[test]
    fn a_winner_must_have_finished_and_a_run_is_decided_once() -> Result<(), PickError> {
        let mut run = run_of(VariantRunId(3), 2);
        launch(&mut run, VariantId(1), UNIX_EPOCH);
        assert_eq!(
            run.plan_pick(VariantId(1), WinnerOutcome::KeepWorktree),
            Err(PickError::NotFinished)
        );
        assert_eq!(
            run.plan_pick(VariantId(7), WinnerOutcome::KeepWorktree),
            Err(PickError::UnknownVariant)
        );
        let mut run = three_finished();
        crate::variants::fixtures::test(&mut run, VariantId(2), TestVerdict::Passed);
        let plan = run.plan_pick(VariantId(2), WinnerOutcome::KeepWorktree)?;
        run.record_pick(&plan);
        let statuses: Vec<VariantStatus> = run.variants.iter().map(|v| v.status.clone()).collect();
        assert_eq!(
            statuses,
            [
                VariantStatus::Discarded,
                VariantStatus::Chosen,
                VariantStatus::Discarded
            ]
        );
        assert_eq!(
            run.plan_pick(VariantId(1), WinnerOutcome::KeepWorktree),
            Err(PickError::AlreadyDecided)
        );
        Ok(())
    }
}
