use crate::agent::{AgentPreset, AgentState};
use crate::environment::{NodeKind, TerminalNode};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const PREFIX_PUNCTUATION: [char; 4] = ['-', '_', '.', '/'];
const PREFIX_FORBIDDEN: [&str; 3] = ["..", "//", "@{"];
const PREFIX_EDGE: char = '/';
pub const ENV_FILE_PREFIX: &str = ".env";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeCleanup {
    #[default]
    Keep,
    RemoveWhenMerged,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeFolder {
    #[default]
    BesideRepository,
    Custom(PathBuf),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeBase {
    #[default]
    CurrentHead,
    DefaultBranch,
}

impl WorktreeBase {
    pub const ALL: [Self; 2] = [Self::CurrentHead, Self::DefaultBranch];

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::CurrentHead => "Current branch",
            Self::DefaultBranch => "Default branch",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BaseFetch {
    #[default]
    Off,
    FetchFirst,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvFiles {
    #[default]
    Skip,
    Copy,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemovalConfirm {
    #[default]
    WhenChanged,
    Never,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub struct BranchPrefix(String);

impl BranchPrefix {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[must_use]
    pub fn branch(&self, name: &str) -> String {
        format!("{}{name}", self.0)
    }
}

impl From<&str> for BranchPrefix {
    fn from(raw: &str) -> Self {
        let kept: String = raw
            .trim()
            .chars()
            .filter(|character| {
                character.is_ascii_alphanumeric() || PREFIX_PUNCTUATION.contains(character)
            })
            .collect();
        let kept = kept.trim_start_matches(PREFIX_EDGE);
        if PREFIX_FORBIDDEN.iter().any(|bad| kept.contains(bad)) {
            return Self::default();
        }
        Self(kept.to_owned())
    }
}

impl From<String> for BranchPrefix {
    fn from(raw: String) -> Self {
        Self::from(raw.as_str())
    }
}

impl From<BranchPrefix> for String {
    fn from(prefix: BranchPrefix) -> Self {
        prefix.0
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorktreeSettings {
    #[serde(default)]
    pub cleanup: WorktreeCleanup,
    #[serde(default)]
    pub folder: WorktreeFolder,
    #[serde(default, skip_serializing_if = "BranchPrefix::is_empty")]
    pub branch_prefix: BranchPrefix,
    #[serde(default)]
    pub base: WorktreeBase,
    #[serde(default)]
    pub fetch: BaseFetch,
    #[serde(default)]
    pub env_files: EnvFiles,
    #[serde(default)]
    pub confirm_removal: RemovalConfirm,
}

impl WorktreeSettings {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[must_use]
pub fn is_env_file(name: &str) -> bool {
    name.starts_with(ENV_FILE_PREFIX)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct WorktreeCheck {
    pub uncommitted: bool,
    pub unmerged_commits: usize,
    pub committed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WorktreeState {
    Dirty,
    Ahead(usize),
    Fresh,
    Merged,
}

impl From<WorktreeCheck> for WorktreeState {
    fn from(check: WorktreeCheck) -> Self {
        if check.uncommitted {
            return Self::Dirty;
        }
        if check.unmerged_commits > 0 {
            return Self::Ahead(check.unmerged_commits);
        }
        if check.committed {
            Self::Merged
        } else {
            Self::Fresh
        }
    }
}

impl WorktreeState {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Dirty => "Uncommitted changes",
            Self::Ahead(_) => "Not merged",
            Self::Fresh => "Clean",
            Self::Merged => "Merged",
        }
    }

    #[must_use]
    pub const fn loses_work(self) -> bool {
        matches!(self, Self::Dirty | Self::Ahead(_))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorktreeOccupancy {
    Empty,
    AgentsFinished,
    InUse,
}

impl WorktreeOccupancy {
    #[must_use]
    pub fn of<'a>(nodes: impl IntoIterator<Item = &'a TerminalNode>) -> Self {
        let mut occupancy = Self::Empty;
        for node in nodes {
            if !is_finished_agent(node) {
                return Self::InUse;
            }
            occupancy = Self::AgentsFinished;
        }
        occupancy
    }
}

fn is_finished_agent(node: &TerminalNode) -> bool {
    let agent = matches!(node.kind, NodeKind::Terminal | NodeKind::Chat)
        && node
            .harness
            .as_ref()
            .is_some_and(|preset| !matches!(preset, AgentPreset::Shell));
    agent && matches!(node.state, AgentState::Idle | AgentState::Done)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentRun {
    NotSeen,
    Finished,
}

#[must_use]
pub fn removes_automatically(
    cleanup: WorktreeCleanup,
    run: AgentRun,
    occupancy: WorktreeOccupancy,
    state: WorktreeState,
) -> bool {
    cleanup == WorktreeCleanup::RemoveWhenMerged
        && run == AgentRun::Finished
        && occupancy != WorktreeOccupancy::InUse
        && state == WorktreeState::Merged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(harness: Option<AgentPreset>, state: AgentState, kind: NodeKind) -> TerminalNode {
        TerminalNode {
            harness,
            state,
            kind,
            id: crate::ids::TerminalId(0),
            title: String::new(),
            resume: None,
            group: None,
            text_scale: crate::text_scale::TextScale::default(),
            file: None,
            extension: None,
            bookmarks: crate::bookmark::ChatBookmarks::default(),
            usage: crate::usage::ChatUsage::default(),
            checkpoints: crate::checkpoint::ChatCheckpoints::default(),
            choice: crate::model::ChatChoice::default(),
            cloud: None,
        }
    }

    const fn check(uncommitted: bool, unmerged_commits: usize, committed: bool) -> WorktreeCheck {
        WorktreeCheck {
            uncommitted,
            unmerged_commits,
            committed,
        }
    }

    #[test]
    fn uncommitted_changes_outrank_every_other_state() {
        assert_eq!(
            WorktreeState::from(check(true, 0, true)),
            WorktreeState::Dirty
        );
        assert_eq!(
            WorktreeState::from(check(true, 3, true)),
            WorktreeState::Dirty
        );
        assert_eq!(
            WorktreeState::from(check(false, 2, true)),
            WorktreeState::Ahead(2)
        );
        assert_eq!(
            WorktreeState::from(check(false, 0, false)),
            WorktreeState::Fresh
        );
        assert_eq!(
            WorktreeState::from(check(false, 0, true)),
            WorktreeState::Merged
        );
    }

    #[test]
    fn auto_remove_never_takes_dirty_unmerged_or_untouched_work() {
        let on = WorktreeCleanup::RemoveWhenMerged;
        let ran = AgentRun::Finished;
        for state in [
            WorktreeState::Dirty,
            WorktreeState::Ahead(1),
            WorktreeState::Fresh,
        ] {
            for occupancy in [WorktreeOccupancy::AgentsFinished, WorktreeOccupancy::Empty] {
                assert!(
                    !removes_automatically(on, ran, occupancy, state),
                    "{state:?}"
                );
            }
        }
        assert!(removes_automatically(
            on,
            ran,
            WorktreeOccupancy::AgentsFinished,
            WorktreeState::Merged
        ));
        assert!(removes_automatically(
            on,
            ran,
            WorktreeOccupancy::Empty,
            WorktreeState::Merged
        ));
    }

    #[test]
    fn auto_remove_waits_for_a_finished_agent_and_is_off_by_default() {
        let merged = WorktreeState::Merged;
        let finished = WorktreeOccupancy::AgentsFinished;
        let on = WorktreeCleanup::RemoveWhenMerged;
        assert!(!removes_automatically(
            WorktreeSettings::default().cleanup,
            AgentRun::Finished,
            finished,
            merged
        ));
        assert!(!removes_automatically(
            on,
            AgentRun::Finished,
            WorktreeOccupancy::InUse,
            merged
        ));
        assert!(!removes_automatically(
            on,
            AgentRun::NotSeen,
            finished,
            merged
        ));
        assert!(!removes_automatically(
            on,
            AgentRun::NotSeen,
            WorktreeOccupancy::Empty,
            merged
        ));
    }

    #[test]
    fn only_finished_agents_leave_a_worktree_free() {
        let done = node(
            Some(AgentPreset::ClaudeCode),
            AgentState::Done,
            NodeKind::Terminal,
        );
        let idle_chat = node(Some(AgentPreset::Codex), AgentState::Idle, NodeKind::Chat);
        let working = node(
            Some(AgentPreset::ClaudeCode),
            AgentState::Working,
            NodeKind::Terminal,
        );
        let waiting = node(
            Some(AgentPreset::Codex),
            AgentState::WaitingForInput,
            NodeKind::Chat,
        );
        let shell = node(
            Some(AgentPreset::Shell),
            AgentState::Idle,
            NodeKind::Terminal,
        );
        let plain = node(None, AgentState::Idle, NodeKind::Terminal);
        let file = node(Some(AgentPreset::Codex), AgentState::Idle, NodeKind::File);
        assert_eq!(WorktreeOccupancy::of([]), WorktreeOccupancy::Empty);
        assert_eq!(
            WorktreeOccupancy::of([&done, &idle_chat]),
            WorktreeOccupancy::AgentsFinished
        );
        for busy in [&working, &waiting, &shell, &plain, &file] {
            assert_eq!(
                WorktreeOccupancy::of([&done, busy]),
                WorktreeOccupancy::InUse
            );
        }
    }

    #[test]
    fn branch_prefixes_keep_only_safe_ref_characters() {
        assert_eq!(BranchPrefix::from("krad/").branch("fix"), "krad/fix");
        assert_eq!(BranchPrefix::from(" /agents- ").as_str(), "agents-");
        assert_eq!(BranchPrefix::from("my team/").as_str(), "myteam/");
        assert!(BranchPrefix::from("a..b").is_empty());
        assert!(BranchPrefix::from("a//b").is_empty());
        assert_eq!(BranchPrefix::default().branch("fix"), "fix");
    }

    #[test]
    fn env_files_are_the_dot_env_family() {
        assert!(is_env_file(".env"));
        assert!(is_env_file(".env.local"));
        assert!(!is_env_file("env"));
    }

    #[test]
    fn worktree_settings_roundtrip_and_old_files_get_defaults() -> serde_json::Result<()> {
        let settings = WorktreeSettings {
            cleanup: WorktreeCleanup::RemoveWhenMerged,
            folder: WorktreeFolder::Custom(PathBuf::from("/work/trees")),
            branch_prefix: BranchPrefix::from("krad/"),
            base: WorktreeBase::DefaultBranch,
            fetch: BaseFetch::FetchFirst,
            env_files: EnvFiles::Copy,
            confirm_removal: RemovalConfirm::Never,
        };
        let back: WorktreeSettings = serde_json::from_str(&serde_json::to_string(&settings)?)?;
        assert_eq!(back, settings);
        let old: WorktreeSettings = serde_json::from_str("{}")?;
        assert!(old.is_default());
        assert_eq!(old.cleanup, WorktreeCleanup::Keep);
        assert_eq!(old.env_files, EnvFiles::Skip);
        assert_eq!(old.confirm_removal, RemovalConfirm::WhenChanged);
        let tampered: WorktreeSettings = serde_json::from_str(r#"{"branch_prefix":"../x"}"#)?;
        assert!(tampered.branch_prefix.is_empty());
        Ok(())
    }

    #[test]
    fn the_app_settings_keep_the_worktree_page_across_a_save() -> serde_json::Result<()> {
        assert!(!serde_json::to_string(&crate::Settings::default())?.contains("worktrees"));
        let mut settings = crate::Settings::default();
        settings.worktrees.cleanup = WorktreeCleanup::RemoveWhenMerged;
        settings.worktrees.env_files = EnvFiles::Copy;
        let back: crate::Settings = serde_json::from_str(&serde_json::to_string(&settings)?)?;
        assert_eq!(back.worktrees, settings.worktrees);
        Ok(())
    }
}
