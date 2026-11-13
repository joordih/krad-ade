mod patch;
mod refs;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::ChangeKind;

pub use patch::{FilePatch, Hunk, HunkLine, HunkLineKind, PatchBody, PatchStatus};
pub use refs::{Branch, BranchPlace, Refs, ORIGIN_REMOTE};

const NAME_SEPARATOR: char = '-';
const ALLOWED_NAME_PUNCTUATION: [char; 3] = ['-', '_', '.'];
const EDGE_TRIMMED: [char; 2] = ['-', '.'];
const PARENT_DIR_MARKER: &str = "..";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileChange {
    pub path: PathBuf,
    pub kind: ChangeKind,
    pub origin: Option<PathBuf>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitStatus {
    pub head: Option<String>,
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub changes: Vec<FileChange>,
    pub staged: Vec<FileChange>,
    pub unstaged: Vec<FileChange>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineCount {
    pub added: u32,
    pub removed: u32,
}

impl std::ops::Add for LineCount {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            added: self.added.saturating_add(other.added),
            removed: self.removed.saturating_add(other.removed),
        }
    }
}

#[must_use]
pub fn sanitize_worktree_name(raw: &str) -> Option<String> {
    let mut name = String::with_capacity(raw.len());
    for character in raw.chars() {
        if character.is_ascii_alphanumeric() || ALLOWED_NAME_PUNCTUATION.contains(&character) {
            name.push(character);
        } else if !name.ends_with(NAME_SEPARATOR) {
            name.push(NAME_SEPARATOR);
        }
    }
    let name = name.trim_matches(EDGE_TRIMMED.as_slice());
    (!name.is_empty() && !name.contains(PARENT_DIR_MARKER)).then(|| name.to_owned())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Worktree {
    pub path: PathBuf,
    pub branch: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupGit {
    pub repo_root: PathBuf,
    pub status: GitStatus,
    pub lines: HashMap<PathBuf, LineCount>,
    pub worktrees: Vec<Worktree>,
    pub refs: Refs,
}

impl GroupGit {
    #[must_use]
    pub fn push_target(&self) -> Option<PushTarget> {
        if self.status.upstream.is_some() {
            return Some(PushTarget::Upstream);
        }
        let branch = self.status.branch.clone()?;
        Some(PushTarget::Publish {
            remote: self.refs.preferred_remote(),
            branch,
        })
    }

    #[must_use]
    pub fn commit_scope(&self) -> CommitScope {
        if self.status.staged.is_empty() {
            CommitScope::Everything
        } else {
            CommitScope::Staged
        }
    }

    #[must_use]
    pub fn total_lines(&self) -> LineCount {
        self.lines
            .values()
            .copied()
            .fold(LineCount::default(), |total, count| total + count)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitMode {
    New,
    Amend,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitScope {
    Staged,
    Everything,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "to", rename_all = "snake_case")]
pub enum PushTarget {
    Upstream,
    Publish { remote: String, branch: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_group_git_read_round_trips_through_json() -> serde_json::Result<()> {
        let git = GroupGit {
            repo_root: PathBuf::from("/r"),
            status: GitStatus {
                branch: Some("main".to_owned()),
                changes: vec![FileChange {
                    path: PathBuf::from("a.rs"),
                    kind: ChangeKind::Modified,
                    origin: None,
                }],
                ..GitStatus::default()
            },
            lines: HashMap::from([(
                PathBuf::from("a.rs"),
                LineCount {
                    added: 2,
                    removed: 1,
                },
            )]),
            worktrees: vec![Worktree {
                path: PathBuf::from("/r"),
                branch: Some("main".to_owned()),
            }],
            refs: Refs::default(),
        };
        let json = serde_json::to_string(&git)?;
        assert_eq!(serde_json::from_str::<GroupGit>(&json)?, git);
        assert_eq!(
            git.total_lines(),
            LineCount {
                added: 2,
                removed: 1
            }
        );
        Ok(())
    }

    #[test]
    fn sanitize_worktree_name_handles_edge_cases() {
        let cases = [
            ("Fix login bug!", Some("Fix-login-bug")),
            ("../x", Some("x")),
            ("feature/auth", Some("feature-auth")),
            ("a..b", None),
            ("---", None),
            ("", None),
        ];
        for (raw, expected) in cases {
            assert_eq!(
                sanitize_worktree_name(raw).as_deref(),
                expected,
                "input {raw:?}"
            );
        }
    }

    fn remote_branch(remote: &str, name: &str) -> Branch {
        Branch {
            name: name.to_owned(),
            place: BranchPlace::Remote(remote.to_owned()),
            upstream: None,
            is_head: false,
        }
    }

    fn on_branch(branch: Option<&str>, upstream: Option<&str>, refs: Refs) -> GroupGit {
        GroupGit {
            repo_root: PathBuf::from("/r"),
            status: GitStatus {
                branch: branch.map(str::to_owned),
                upstream: upstream.map(str::to_owned),
                ..GitStatus::default()
            },
            lines: HashMap::new(),
            worktrees: Vec::new(),
            refs,
        }
    }

    #[test]
    fn a_new_branch_is_published_to_origin_or_else_the_first_remote() {
        let fork = Refs {
            branches: vec![
                remote_branch("upstream", "main"),
                remote_branch("fork", "x"),
            ],
            tags: Vec::new(),
        };
        assert_eq!(
            on_branch(Some("fix"), None, fork).push_target(),
            Some(PushTarget::Publish {
                remote: "fork".to_owned(),
                branch: "fix".to_owned(),
            })
        );
        let both = Refs {
            branches: vec![
                remote_branch("fork", "main"),
                remote_branch(ORIGIN_REMOTE, "main"),
            ],
            tags: Vec::new(),
        };
        assert_eq!(both.preferred_remote(), ORIGIN_REMOTE);
        assert_eq!(Refs::default().preferred_remote(), ORIGIN_REMOTE);
        assert_eq!(
            on_branch(Some("fix"), Some("origin/fix"), Refs::default()).push_target(),
            Some(PushTarget::Upstream)
        );
        assert_eq!(on_branch(None, None, Refs::default()).push_target(), None);
    }

    #[test]
    fn a_push_target_names_where_it_goes() -> serde_json::Result<()> {
        let publish = PushTarget::Publish {
            remote: ORIGIN_REMOTE.to_owned(),
            branch: "fix".to_owned(),
        };
        assert_eq!(
            serde_json::to_string(&publish)?,
            r#"{"to":"publish","remote":"origin","branch":"fix"}"#
        );
        assert_eq!(
            serde_json::to_string(&PushTarget::Upstream)?,
            r#"{"to":"upstream"}"#
        );
        Ok(())
    }
}
