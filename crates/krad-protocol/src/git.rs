use krad_domain::{
    Branch, CommitMode, EnvironmentName, FileChange, FilePatch, GroupGit, GroupId, Host,
    HunkFingerprint, SessionId,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::ids::RequestId;

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct GitCommand {
    pub environment: EnvironmentName,
    pub group: GroupId,
    pub act: GitAct,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum GitAct {
    Refresh,
    AgentsFinished,
    Fetch,
    Pull,
    Push,
    Sync,
    PushTags,
    Stash,
    PopStash,
    Stage {
        paths: Vec<PathBuf>,
    },
    Unstage {
        paths: Vec<PathBuf>,
    },
    StageAll,
    UnstageAll,
    Commit {
        message: String,
        mode: CommitMode,
        push: bool,
    },
    CreateBranch {
        name: String,
    },
    CreateTag {
        name: String,
    },
    DeleteTag {
        name: String,
    },
    CheckOut {
        branch: Branch,
    },
    Discard {
        change: FileChange,
    },
    DiscardFolder {
        folder: PathBuf,
    },
    CreateWorktree {
        name: String,
    },
    OpenBranchWorktree {
        branch: Branch,
    },
    RemoveWorktree,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ChangesCommand {
    pub environment: EnvironmentName,
    pub session: SessionId,
    pub act: ChangesAct,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum ChangesAct {
    Capture,
    Read,
    Restart,
    Revert {
        patch: Box<FilePatch>,
        hunk: Option<usize>,
    },
    Accept {
        hunks: Vec<HunkFingerprint>,
    },
    Unaccept {
        hunks: Vec<HunkFingerprint>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct GitRead {
    pub host: Host,
    pub root: PathBuf,
    pub git: Option<Box<GroupGit>>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum GitOutcome {
    Done,
    Opened {
        group: GroupId,
    },
    Stale,
    Failed {
        message: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct GitFinished {
    pub request: RequestId,
    pub environment: EnvironmentName,
    pub outcome: GitOutcome,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct WorktreeSwept {
    pub environment: EnvironmentName,
    pub group: GroupId,
    pub name: String,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "read", rename_all = "snake_case")]
pub enum ChangesRead {
    NoRepository,
    Failed {
        message: String,
    },
    Ready {
        repo: PathBuf,
        files: Vec<FilePatch>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SessionChangesRead {
    pub request: RequestId,
    pub environment: EnvironmentName,
    pub session: SessionId,
    pub read: ChangesRead,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Event, EventKind, Seq};

    fn main() -> EnvironmentName {
        EnvironmentName::new("main".to_owned())
    }

    #[test]
    fn git_commands_have_a_stable_wire_shape() -> serde_json::Result<()> {
        let command = Command::Git(GitCommand {
            environment: main(),
            group: GroupId(2),
            act: GitAct::Commit {
                message: "fix".to_owned(),
                mode: CommitMode::New,
                push: true,
            },
        });
        let json = serde_json::to_string(&command)?;
        assert_eq!(
            json,
            r#"{"area":"git","environment":"main","group":2,"act":{"verb":"commit","message":"fix","mode":"new","push":true}}"#
        );
        assert_eq!(serde_json::from_str::<Command>(&json)?, command);
        let newer: Command = serde_json::from_str(
            r#"{"area":"git","environment":"main","group":2,"act":{"verb":"rebase"}}"#,
        )?;
        assert!(matches!(
            newer,
            Command::Git(GitCommand {
                act: GitAct::Unknown,
                ..
            })
        ));
        Ok(())
    }

    #[test]
    fn session_changes_round_trip_and_newer_reads_are_unknown() -> serde_json::Result<()> {
        let command = Command::Changes(ChangesCommand {
            environment: main(),
            session: SessionId(4),
            act: ChangesAct::Restart,
        });
        let json = serde_json::to_string(&command)?;
        assert_eq!(
            json,
            r#"{"area":"changes","environment":"main","session":4,"act":{"verb":"restart"}}"#
        );
        assert_eq!(serde_json::from_str::<Command>(&json)?, command);
        let event = Event {
            seq: Seq(1),
            kind: EventKind::SessionChangesRead(SessionChangesRead {
                request: RequestId(3),
                environment: main(),
                session: SessionId(4),
                read: ChangesRead::NoRepository,
            }),
        };
        let json = serde_json::to_string(&event)?;
        assert_eq!(serde_json::from_str::<Event>(&json)?, event);
        let newer: ChangesRead = serde_json::from_str(r#"{"read":"partial"}"#)?;
        assert_eq!(newer, ChangesRead::Unknown);
        Ok(())
    }

    #[test]
    fn a_git_read_tells_its_root_and_a_finish_its_request() -> serde_json::Result<()> {
        let events = [
            EventKind::GitRead(GitRead {
                host: Host::Local,
                root: PathBuf::from("/r"),
                git: None,
            }),
            EventKind::GitFinished(GitFinished {
                request: RequestId(9),
                environment: main(),
                outcome: GitOutcome::Opened { group: GroupId(3) },
            }),
        ];
        for kind in events {
            let event = Event { seq: Seq(2), kind };
            let json = serde_json::to_string(&event)?;
            assert_eq!(serde_json::from_str::<Event>(&json)?, event);
        }
        let newer: GitOutcome = serde_json::from_str(r#"{"outcome":"conflicted"}"#)?;
        assert_eq!(newer, GitOutcome::Unknown);
        Ok(())
    }
}
