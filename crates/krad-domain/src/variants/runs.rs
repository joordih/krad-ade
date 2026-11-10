use super::order::VariantsOrder;
use super::run::{VariantId, VariantRun, VariantRunId};
use super::status::{VariantEvent, VariantStatus};
use crate::ids::{GroupId, TerminalId};
use crate::usage::UsdCost;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub const MAX_KEPT_RUNS: usize = 8;
const FIRST_RUN: u64 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariantRuns {
    #[serde(default)]
    runs: Vec<VariantRun>,
    #[serde(default = "first_run")]
    next: u64,
}

const fn first_run() -> u64 {
    FIRST_RUN
}

impl Default for VariantRuns {
    fn default() -> Self {
        Self {
            runs: Vec::new(),
            next: FIRST_RUN,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartRunError {
    TooManyActiveRuns,
}

impl fmt::Display for StartRunError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TooManyActiveRuns => {
                "This group already keeps the most variant runs it can, and every one is still \
                 running or holds worktrees; pick a winner or discard a run first"
            }
        })
    }
}

impl std::error::Error for StartRunError {}

impl VariantRuns {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }

    /// # Errors
    ///
    /// Returns [`StartRunError::TooManyActiveRuns`] when the group already keeps
    /// [`MAX_KEPT_RUNS`] runs and none of them can be let go.
    pub fn start(
        &mut self,
        group: GroupId,
        base_root: PathBuf,
        order: VariantsOrder,
        now: SystemTime,
    ) -> Result<VariantRunId, StartRunError> {
        self.make_room(group)?;
        let after_kept = self.runs.iter().map(|run| run.id.0 + 1).max();
        let id = VariantRunId(
            self.next
                .max(FIRST_RUN)
                .max(after_kept.unwrap_or(FIRST_RUN)),
        );
        self.next = id.0 + 1;
        self.runs
            .push(VariantRun::new(id, group, base_root, order, now));
        Ok(id)
    }

    fn make_room(&mut self, group: GroupId) -> Result<(), StartRunError> {
        while self.of_group(group).count() >= MAX_KEPT_RUNS {
            let oldest = self
                .of_group(group)
                .find(|run| run.can_be_let_go())
                .map(|run| run.id)
                .ok_or(StartRunError::TooManyActiveRuns)?;
            self.runs.retain(|run| run.id != oldest);
        }
        Ok(())
    }

    pub fn of_group(&self, group: GroupId) -> impl Iterator<Item = &VariantRun> {
        self.runs.iter().filter(move |run| run.group == group)
    }

    #[must_use]
    pub fn all(&self) -> &[VariantRun] {
        &self.runs
    }

    #[must_use]
    pub fn latest(&self, group: GroupId) -> Option<&VariantRun> {
        self.of_group(group).last()
    }

    #[must_use]
    pub fn run(&self, id: VariantRunId) -> Option<&VariantRun> {
        self.runs.iter().find(|run| run.id == id)
    }

    pub fn run_mut(&mut self, id: VariantRunId) -> Option<&mut VariantRun> {
        self.runs.iter_mut().find(|run| run.id == id)
    }

    #[must_use]
    pub fn of_terminal(&self, terminal: TerminalId) -> Option<(VariantRunId, VariantId)> {
        self.runs.iter().find_map(|run| {
            run.variants
                .iter()
                .filter(|variant| !variant.status.is_closed())
                .find(|variant| variant.terminal == Some(terminal))
                .map(|variant| (run.id, variant.id))
        })
    }

    pub fn open_terminals(
        &self,
    ) -> impl Iterator<Item = (VariantRunId, VariantId, TerminalId)> + '_ {
        self.runs.iter().flat_map(|run| {
            run.variants
                .iter()
                .filter(|variant| !variant.status.is_closed())
                .filter_map(move |variant| Some((run.id, variant.id, variant.terminal?)))
        })
    }

    #[must_use]
    pub fn average_variant_cost(&self, group: GroupId) -> Option<UsdCost> {
        let costs: Vec<UsdCost> = self
            .of_group(group)
            .flat_map(|run| &run.variants)
            .filter(|variant| variant.finished_at.is_some() && !variant.spent.cost.is_zero())
            .map(|variant| variant.spent.cost)
            .collect();
        let count = u64::try_from(costs.len()).ok().filter(|count| *count > 0)?;
        let total: UsdCost = costs.into_iter().sum();
        Some(UsdCost::from_nanos(total.nanos() / count))
    }

    pub fn mark_missing_worktrees(
        &mut self,
        exists: impl Fn(&Path) -> bool,
    ) -> Vec<(VariantRunId, VariantId)> {
        let mut missing = Vec::new();
        for run in &mut self.runs {
            for variant in &mut run.variants {
                let gone = !variant.status.is_closed()
                    && variant.status != VariantStatus::Missing
                    && variant
                        .worktree
                        .as_ref()
                        .is_some_and(|worktree| !exists(&worktree.path));
                if gone && variant.apply(VariantEvent::WorktreeMissing).is_ok() {
                    missing.push((run.id, variant.id));
                }
            }
        }
        missing
    }

    pub fn forget_group(&mut self, group: GroupId) {
        self.runs.retain(|run| run.group != group);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::{Spend, TokenUsage};
    use crate::variants::fixtures::{finish, launch, sample_run, worktree_of, BASE_GROUP};
    use crate::variants::{VariantSpec, VariantStatus, WinnerOutcome};
    use crate::AgentPreset;
    use std::time::UNIX_EPOCH;

    fn order() -> VariantsOrder {
        let spec = VariantSpec::new(AgentPreset::Codex, None, Some("Be terse"));
        VariantsOrder {
            prompt: "Add a flag".to_owned(),
            variants: vec![spec.clone(), spec],
            test: None,
        }
    }

    fn try_start(runs: &mut VariantRuns, group: GroupId) -> Result<VariantRunId, StartRunError> {
        runs.start(group, PathBuf::from("/repo"), order(), UNIX_EPOCH)
    }

    fn start(runs: &mut VariantRuns, group: GroupId) -> VariantRunId {
        let Ok(id) = try_start(runs, group) else {
            unreachable!("the test keeps room for a new run");
        };
        id
    }

    fn decide(runs: &mut VariantRuns, id: VariantRunId) {
        if let Some(run) = runs.run_mut(id) {
            launch(run, VariantId(1), UNIX_EPOCH);
            finish(run, VariantId(1), UNIX_EPOCH);
            let plan = run.plan_pick(VariantId(1), WinnerOutcome::KeepWorktree);
            if let Ok(plan) = plan {
                run.record_pick(&plan);
            }
        }
    }

    fn fail_all(runs: &mut VariantRuns, id: VariantRunId) {
        if let Some(run) = runs.run_mut(id) {
            for variant in &mut run.variants {
                assert!(variant
                    .apply(VariantEvent::LaunchFailed("no worktree".to_owned()))
                    .is_ok());
            }
        }
    }

    #[test]
    fn runs_survive_a_save_and_load() -> serde_json::Result<()> {
        let mut runs = VariantRuns::default();
        let id = start(&mut runs, BASE_GROUP);
        if let Some(run) = runs.run_mut(id) {
            launch(run, VariantId(1), UNIX_EPOCH);
            finish(run, VariantId(1), UNIX_EPOCH);
            run.variants[0].spent = Spend {
                tokens: TokenUsage {
                    input: 12,
                    ..TokenUsage::default()
                },
                cost: UsdCost::from_nanos(7),
            };
        }
        let saved = serde_json::to_string(&runs)?;
        let loaded: VariantRuns = serde_json::from_str(&saved)?;
        assert_eq!(loaded, runs);
        assert_eq!(
            serde_json::from_str::<VariantRuns>("{}")?,
            VariantRuns::default()
        );
        Ok(())
    }

    #[test]
    fn each_group_keeps_a_bounded_number_of_runs_dropping_only_settled_ones() {
        let mut runs = VariantRuns::default();
        let running = start(&mut runs, BASE_GROUP);
        let decided = start(&mut runs, BASE_GROUP);
        decide(&mut runs, decided);
        let failed = start(&mut runs, BASE_GROUP);
        fail_all(&mut runs, failed);
        let other = start(&mut runs, GroupId(9));
        for _ in 3..MAX_KEPT_RUNS {
            start(&mut runs, BASE_GROUP);
        }
        assert_eq!(runs.of_group(BASE_GROUP).count(), MAX_KEPT_RUNS);
        let newest = start(&mut runs, BASE_GROUP);
        assert_eq!(runs.of_group(BASE_GROUP).count(), MAX_KEPT_RUNS);
        assert!(runs.run(decided).is_none());
        assert!(runs.run(running).is_some());
        assert!(runs.run(other).is_some());
        assert_eq!(runs.latest(BASE_GROUP).map(|run| run.id), Some(newest));
        let next = start(&mut runs, BASE_GROUP);
        assert!(runs.run(failed).is_none());
        assert!(runs.run(running).is_some());
        assert!(next.0 > newest.0);
    }

    #[test]
    fn a_new_run_is_refused_when_every_kept_run_is_still_active() {
        let mut runs = VariantRuns::default();
        let comparing = start(&mut runs, BASE_GROUP);
        if let Some(run) = runs.run_mut(comparing) {
            for id in 1..=2 {
                launch(run, VariantId(id), UNIX_EPOCH);
                finish(run, VariantId(id), UNIX_EPOCH);
            }
        }
        for _ in 1..MAX_KEPT_RUNS {
            start(&mut runs, BASE_GROUP);
        }
        let kept: Vec<VariantRunId> = runs.of_group(BASE_GROUP).map(|run| run.id).collect();
        assert_eq!(
            try_start(&mut runs, BASE_GROUP),
            Err(StartRunError::TooManyActiveRuns)
        );
        let after: Vec<VariantRunId> = runs.of_group(BASE_GROUP).map(|run| run.id).collect();
        assert_eq!(after, kept);
        assert!(try_start(&mut runs, GroupId(9)).is_ok());
    }

    #[test]
    fn a_new_run_never_reuses_the_id_of_a_kept_run() {
        let mut runs = VariantRuns {
            runs: vec![sample_run()],
            next: FIRST_RUN,
        };
        let id = start(&mut runs, BASE_GROUP);
        assert!(id > runs.all()[0].id);
    }

    #[test]
    fn worktrees_deleted_outside_the_app_mark_their_variants_missing() {
        let mut runs = VariantRuns {
            runs: vec![sample_run()],
            next: 6,
        };
        let id = runs.all()[0].id;
        if let Some(run) = runs.run_mut(id) {
            launch(run, VariantId(1), UNIX_EPOCH);
            launch(run, VariantId(2), UNIX_EPOCH);
        }
        let kept = runs
            .run(id)
            .map_or_else(PathBuf::new, |run| worktree_of(run, VariantId(2)).path);
        let missing = runs.mark_missing_worktrees(|path| path == kept);
        assert_eq!(missing, [(id, VariantId(1))]);
        let status = runs.run(id).map(|run| run.variants[0].status.clone());
        assert_eq!(status, Some(VariantStatus::Missing));
        assert_eq!(runs.mark_missing_worktrees(|path| path == kept), []);
    }

    #[test]
    fn terminals_lead_back_to_their_variant_and_costs_average_over_finished_ones() {
        let mut runs = VariantRuns::default();
        let id = start(&mut runs, BASE_GROUP);
        if let Some(run) = runs.run_mut(id) {
            launch(run, VariantId(1), UNIX_EPOCH);
            launch(run, VariantId(2), UNIX_EPOCH);
            finish(run, VariantId(1), UNIX_EPOCH);
            finish(run, VariantId(2), UNIX_EPOCH);
            run.variants[0].spent.cost = UsdCost::from_nanos(100);
            run.variants[1].spent.cost = UsdCost::from_nanos(300);
        }
        assert_eq!(runs.of_terminal(TerminalId(102)), Some((id, VariantId(2))));
        assert_eq!(runs.open_terminals().count(), 2);
        assert_eq!(
            runs.average_variant_cost(BASE_GROUP),
            Some(UsdCost::from_nanos(200))
        );
        assert_eq!(runs.average_variant_cost(GroupId(9)), None);
    }
}
