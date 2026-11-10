mod budget;
#[cfg(test)]
mod fixtures;
mod order;
mod pick;
mod run;
mod runs;
mod status;
mod totals;

pub use budget::{launch_budget_warning, LaunchBudgetWarning, LaunchEstimate};
pub use order::{VariantSpec, VariantsOrder, VariantsOrderError, MAX_VARIANTS, MIN_VARIANTS};
pub use pick::{ApplyStep, CleanupPlan, DiscardStep, PickError, WinnerOutcome};
pub use run::{
    ChangedLines, DiffSummary, RunDecision, RunPhase, Variant, VariantId, VariantRun, VariantRunId,
    VariantWorktree, MAX_DIFF_FILES,
};
pub use runs::{StartRunError, VariantRuns, MAX_KEPT_RUNS};
pub use status::{
    TestOutcome, TestVerdict, TransitionError, VariantEvent, VariantStatus, MAX_TEST_TAIL_BYTES,
};
pub use totals::RunTotals;
