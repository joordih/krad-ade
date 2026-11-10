use super::order::{VariantSpec, VariantsOrder};
use super::run::{VariantId, VariantRun, VariantRunId, VariantWorktree};
use super::status::{TestOutcome, TestVerdict, VariantEvent};
use crate::agent::AgentPreset;
use crate::ids::{GroupId, TerminalId};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(super) const BASE_GROUP: GroupId = GroupId(1);

pub(super) fn sample_run() -> VariantRun {
    run_of(VariantRunId(5), 2)
}

pub(super) fn run_of(id: VariantRunId, variants: usize) -> VariantRun {
    let spec = VariantSpec::new(AgentPreset::ClaudeCode, None, None);
    let order = VariantsOrder {
        prompt: "Fix the login".to_owned(),
        variants: vec![spec; variants],
        test: None,
    };
    VariantRun::new(id, BASE_GROUP, PathBuf::from("/repo"), order, UNIX_EPOCH)
}

pub(super) fn worktree_of(run: &VariantRun, variant: VariantId) -> VariantWorktree {
    let name = run.worktree_name(variant);
    VariantWorktree {
        path: PathBuf::from("/repo.worktrees").join(&name),
        branch: name,
        group: GroupId(10 + u64::from(variant.0)),
    }
}

pub(super) fn launch(run: &mut VariantRun, variant: VariantId, at: SystemTime) {
    let worktree = worktree_of(run, variant);
    let terminal = TerminalId(100 + u64::from(variant.0));
    if let Some(target) = run.variant_mut(variant) {
        let launched = VariantEvent::Launched {
            worktree,
            terminal,
            at,
        };
        assert!(target.apply(launched).is_ok());
    }
}

pub(super) fn finish(run: &mut VariantRun, variant: VariantId, at: SystemTime) {
    if let Some(target) = run.variant_mut(variant) {
        assert!(target.apply(VariantEvent::Finished { at }).is_ok());
    }
}

pub(super) fn test(run: &mut VariantRun, variant: VariantId, verdict: TestVerdict) {
    if let Some(target) = run.variant_mut(variant) {
        assert!(target.apply(VariantEvent::TestsStarted).is_ok());
        let outcome = TestOutcome::new(verdict, Duration::from_secs(1), "");
        assert!(target.apply(VariantEvent::TestsDone(outcome)).is_ok());
    }
}
