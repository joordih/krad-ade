use super::order::{VariantSpec, VariantsOrder};
use super::pick::WinnerOutcome;
use super::status::{TestOutcome, TransitionError, VariantEvent, VariantStatus};
use crate::ids::{GroupId, TerminalId};
use crate::project_task::ProjectTask;
use crate::usage::Spend;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

pub const MAX_DIFF_FILES: usize = 200;
const WORKTREE_PREFIX: &str = "variant";
const VARIANT_MARK: &str = "#";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VariantRunId(pub u64);

impl fmt::Display for VariantRunId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VariantId(pub u32);

impl fmt::Display for VariantId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{VARIANT_MARK}{}", self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariantWorktree {
    pub path: PathBuf,
    pub branch: String,
    pub group: GroupId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangedLines {
    pub path: PathBuf,
    pub added: u32,
    pub removed: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffSummary {
    pub files: Vec<ChangedLines>,
    #[serde(default)]
    pub more_files: usize,
    pub added: u64,
    pub removed: u64,
}

impl DiffSummary {
    #[must_use]
    pub fn new(mut files: Vec<ChangedLines>) -> Self {
        let added = files.iter().map(|file| u64::from(file.added)).sum();
        let removed = files.iter().map(|file| u64::from(file.removed)).sum();
        files.sort_by(|left, right| left.path.cmp(&right.path));
        let more_files = files.len().saturating_sub(MAX_DIFF_FILES);
        files.truncate(MAX_DIFF_FILES);
        Self {
            files,
            more_files,
            added,
            removed,
        }
    }

    #[must_use]
    pub fn file_count(&self) -> usize {
        self.files.len() + self.more_files
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Variant {
    pub id: VariantId,
    pub spec: VariantSpec,
    #[serde(flatten)]
    pub status: VariantStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<VariantWorktree>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal: Option<TerminalId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<SystemTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<SystemTime>,
    #[serde(default, skip_serializing_if = "Spend::is_empty")]
    pub spent: Spend,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<DiffSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test: Option<TestOutcome>,
}

impl Variant {
    fn new(id: VariantId, spec: VariantSpec) -> Self {
        Self {
            id,
            spec,
            status: VariantStatus::Launching,
            worktree: None,
            terminal: None,
            started_at: None,
            finished_at: None,
            spent: Spend::default(),
            diff: None,
            test: None,
        }
    }

    /// # Errors
    ///
    /// As [`VariantStatus::next`]; the variant is left unchanged.
    pub fn apply(&mut self, event: VariantEvent) -> Result<(), TransitionError> {
        self.status = self.status.next(&event)?;
        match event {
            VariantEvent::Launched {
                worktree,
                terminal,
                at,
            } => {
                self.worktree = Some(worktree);
                self.terminal = Some(terminal);
                self.started_at = Some(at);
            }
            VariantEvent::Working => {
                self.finished_at = None;
                self.test = None;
            }
            VariantEvent::Finished { at } => self.finished_at = Some(at),
            VariantEvent::TestsDone(outcome) => self.test = Some(outcome),
            VariantEvent::LaunchFailed(_)
            | VariantEvent::TestsStarted
            | VariantEvent::WorktreeMissing
            | VariantEvent::Discarded
            | VariantEvent::Chosen => {}
        }
        Ok(())
    }

    #[must_use]
    pub fn elapsed(&self, now: SystemTime) -> Option<Duration> {
        let started = self.started_at?;
        let ended = self.finished_at.unwrap_or(now);
        Some(ended.duration_since(started).unwrap_or_default())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunDecision {
    pub winner: VariantId,
    pub outcome: WinnerOutcome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunPhase {
    Running,
    Comparing,
    Decided,
}

impl RunPhase {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Running => "Running",
            Self::Comparing => "Ready to compare",
            Self::Decided => "Decided",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariantRun {
    pub id: VariantRunId,
    pub group: GroupId,
    pub base_root: PathBuf,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test: Option<ProjectTask>,
    pub created_at: SystemTime,
    pub variants: Vec<Variant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<RunDecision>,
    #[serde(default)]
    pub announced_finished: bool,
    #[serde(skip)]
    pub(super) pending_pick: Option<VariantId>,
}

impl VariantRun {
    #[must_use]
    pub fn new(
        id: VariantRunId,
        group: GroupId,
        base_root: PathBuf,
        order: VariantsOrder,
        now: SystemTime,
    ) -> Self {
        let variants = (1..)
            .zip(order.variants)
            .map(|(number, spec)| Variant::new(VariantId(number), spec))
            .collect();
        Self {
            id,
            group,
            base_root,
            prompt: order.prompt,
            test: order.test,
            created_at: now,
            variants,
            decision: None,
            announced_finished: false,
            pending_pick: None,
        }
    }

    #[must_use]
    pub fn worktree_name(&self, variant: VariantId) -> String {
        format!("{WORKTREE_PREFIX}-{}-{}", self.id.0, variant.0)
    }

    #[must_use]
    pub fn variant(&self, id: VariantId) -> Option<&Variant> {
        self.variants.iter().find(|variant| variant.id == id)
    }

    pub fn variant_mut(&mut self, id: VariantId) -> Option<&mut Variant> {
        self.variants.iter_mut().find(|variant| variant.id == id)
    }

    #[must_use]
    pub fn phase(&self) -> RunPhase {
        if self.decision.is_some() {
            return RunPhase::Decided;
        }
        if self
            .variants
            .iter()
            .all(|variant| variant.status.is_settled())
        {
            RunPhase::Comparing
        } else {
            RunPhase::Running
        }
    }

    #[must_use]
    pub fn holds_worktrees(&self) -> bool {
        self.variants
            .iter()
            .any(|variant| !variant.status.is_closed() && variant.worktree.is_some())
    }

    #[must_use]
    pub fn can_be_let_go(&self) -> bool {
        self.phase() != RunPhase::Running && !self.holds_worktrees()
    }

    #[must_use]
    pub fn take_finished_announcement(&mut self) -> bool {
        if self.announced_finished || self.phase() != RunPhase::Comparing {
            return false;
        }
        self.announced_finished = true;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variants::fixtures::sample_run;
    use std::time::UNIX_EPOCH;

    const MINUTE: Duration = Duration::from_mins(1);

    #[test]
    fn variants_are_numbered_from_one_and_named_after_their_run() {
        let run = sample_run();
        let ids: Vec<String> = run.variants.iter().map(|v| v.id.to_string()).collect();
        assert_eq!(ids, ["#1", "#2"]);
        assert_eq!(run.worktree_name(VariantId(2)), "variant-5-2");
    }

    #[test]
    fn a_finished_variant_keeps_its_time_and_one_still_working_counts_up() {
        let mut variant = sample_run().variants.remove(0);
        assert_eq!(variant.elapsed(UNIX_EPOCH + MINUTE), None);
        let worktree = VariantWorktree {
            path: PathBuf::from("/w"),
            branch: "b".to_owned(),
            group: GroupId(2),
        };
        let launched = VariantEvent::Launched {
            worktree,
            terminal: TerminalId(4),
            at: UNIX_EPOCH,
        };
        assert!(variant.apply(launched).is_ok());
        assert_eq!(variant.elapsed(UNIX_EPOCH + MINUTE), Some(MINUTE));
        let finished = VariantEvent::Finished {
            at: UNIX_EPOCH + 2 * MINUTE,
        };
        assert!(variant.apply(finished).is_ok());
        assert_eq!(variant.elapsed(UNIX_EPOCH + 9 * MINUTE), Some(2 * MINUTE));
    }

    #[test]
    fn the_run_announces_once_when_every_variant_has_settled() {
        let mut run = sample_run();
        assert_eq!(run.phase(), RunPhase::Running);
        for variant in &mut run.variants {
            assert!(variant
                .apply(VariantEvent::LaunchFailed("gone".to_owned()))
                .is_ok());
        }
        assert_eq!(run.phase(), RunPhase::Comparing);
        assert!(run.take_finished_announcement());
        assert!(!run.take_finished_announcement());
    }

    #[test]
    fn a_diff_summary_counts_every_file_but_keeps_a_bounded_list() {
        let files = (0..MAX_DIFF_FILES + 3)
            .map(|index| ChangedLines {
                path: PathBuf::from(format!("src/{index:04}.rs")),
                added: 2,
                removed: 1,
            })
            .collect();
        let summary = DiffSummary::new(files);
        assert_eq!(summary.files.len(), MAX_DIFF_FILES);
        assert_eq!(summary.file_count(), MAX_DIFF_FILES + 3);
        assert_eq!(summary.added, 2 * (MAX_DIFF_FILES as u64 + 3));
    }
}
