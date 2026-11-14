use krad_domain::review::CommitSha;
use krad_domain::{EnvironmentName, FileChange, GitObjectId, WorktreeBase};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RepoCommand {
    pub environment: EnvironmentName,
    pub ask: RepoAsk,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum RepoAsk {
    DocsSources {
        root: PathBuf,
    },
    SaveDocsSources {
        root: PathBuf,
        sources: Vec<String>,
    },
    SessionTemplates {
        root: PathBuf,
    },
    AddWorktree {
        repo: PathBuf,
        name: String,
    },
    WorktreeStates {
        base: WorktreeBase,
        worktrees: Vec<WorktreeProbe>,
    },
    RemoveWorktree {
        repo: PathBuf,
        path: PathBuf,
        removal: WorktreeRemoval,
    },
    RemoveSettledWorktree {
        repo: PathBuf,
        path: PathBuf,
    },
    RemoteUrls {
        roots: Vec<PathBuf>,
        remote: String,
    },
    CommitLog {
        repo: PathBuf,
        revisions: Vec<String>,
        limit: usize,
    },
    CommitFiles {
        repo: PathBuf,
        sha: CommitSha,
    },
    Diff {
        repo: PathBuf,
        of: DiffOf,
    },
    DiffsBetween {
        repo: PathBuf,
        base: CommitSha,
        head: CommitSha,
        files: Vec<Vec<String>>,
    },
    Tasks {
        root: PathBuf,
    },
    AddNote {
        root: PathBuf,
        folder: String,
        stem: String,
        text: String,
    },
    CheckpointChanges {
        repo: PathBuf,
        checkpoint: GitObjectId,
    },
    RestoreCheckpoint {
        repo: PathBuf,
        checkpoint: GitObjectId,
    },
    ReadText {
        path: PathBuf,
        max_bytes: u64,
    },
    ReplaceText {
        path: PathBuf,
        expected: String,
        text: String,
    },
    Skills {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        root: Option<PathBuf>,
    },
    ChatSkills {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        root: Option<PathBuf>,
    },
    Reveal {
        path: PathBuf,
    },
    Folders {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dir: Option<PathBuf>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DiffOf {
    Working { change: FileChange },
    Commit { sha: CommitSha, change: FileChange },
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct WorktreeProbe {
    pub main: PathBuf,
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeRemoval {
    Clean,
    Discard,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use krad_domain::ChangeKind;

    #[test]
    fn a_repo_ask_names_its_environment_and_verb_on_the_wire() -> serde_json::Result<()> {
        let command = Command::Repo(RepoCommand {
            environment: EnvironmentName::new("main".to_owned()),
            ask: RepoAsk::Diff {
                repo: PathBuf::from("/work/app"),
                of: DiffOf::Working {
                    change: FileChange {
                        path: PathBuf::from("src/main.rs"),
                        kind: ChangeKind::Modified,
                        origin: None,
                    },
                },
            },
        });
        let json = serde_json::to_value(&command)?;
        assert_eq!(json["area"], "repo");
        assert_eq!(json["ask"]["verb"], "diff");
        assert_eq!(json["ask"]["of"]["kind"], "working");
        assert_eq!(serde_json::from_value::<Command>(json)?, command);
        let newer: RepoAsk = serde_json::from_value(serde_json::json!({ "verb": "blame" }))?;
        assert_eq!(newer, RepoAsk::Unknown);
        Ok(())
    }
}
