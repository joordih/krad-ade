use std::path::{Path, PathBuf};

use crate::git::run_git;
use crate::RepoError;

const STATUS_ARGS: [&str; 5] = [
    "status",
    "--porcelain=v2",
    "--branch",
    "--untracked-files=all",
    "-z",
];

const RECORD_SEPARATOR: u8 = 0;
const FIELD_SEPARATOR: char = ' ';

const HEADER_TAG: &str = "#";
const ORDINARY_TAG: &str = "1";
const RENAMED_TAG: &str = "2";
const UNMERGED_TAG: &str = "u";
const UNTRACKED_TAG: &str = "?";

const ORDINARY_FIELDS: usize = 9;
const RENAMED_FIELDS: usize = 10;
const UNMERGED_FIELDS: usize = 11;

const BRANCH_HEAD: &str = "branch.head";
const BRANCH_UPSTREAM: &str = "branch.upstream";
const BRANCH_AHEAD_BEHIND: &str = "branch.ab";
const DETACHED_HEAD: &str = "(detached)";
const AHEAD_PREFIX: char = '+';
const BEHIND_PREFIX: char = '-';

const ADDED_CODE: char = 'A';
const DELETED_CODE: char = 'D';
const RENAMED_CODE: char = 'R';

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
    Conflicted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileChange {
    pub path: PathBuf,
    pub kind: ChangeKind,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GitStatus {
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub changes: Vec<FileChange>,
}

/// # Errors
pub fn status(repo: &Path) -> Result<GitStatus, RepoError> {
    run_git(repo, &STATUS_ARGS).map(|stdout| parse_status(&stdout))
}

#[must_use]
pub fn parse_status(output: &[u8]) -> GitStatus {
    let mut status = GitStatus::default();
    let mut records = output
        .split(|&byte| byte == RECORD_SEPARATOR)
        .map(String::from_utf8_lossy);
    while let Some(record) = records.next() {
        let Some((tag, rest)) = record.split_once(FIELD_SEPARATOR) else {
            continue;
        };
        match tag {
            HEADER_TAG => apply_header(&mut status, rest),
            ORDINARY_TAG => push_change(&mut status, &record, ORDINARY_FIELDS, change_kind),
            RENAMED_TAG => {
                push_change(&mut status, &record, RENAMED_FIELDS, change_kind);
                records.next();
            }
            UNMERGED_TAG => {
                push_change(&mut status, &record, UNMERGED_FIELDS, |_| {
                    ChangeKind::Conflicted
                });
            }
            UNTRACKED_TAG => status.changes.push(FileChange {
                path: PathBuf::from(rest),
                kind: ChangeKind::Untracked,
            }),
            _ => {}
        }
    }
    status
}

fn apply_header(status: &mut GitStatus, header: &str) {
    let Some((key, value)) = header.split_once(FIELD_SEPARATOR) else {
        return;
    };
    match key {
        BRANCH_HEAD => status.branch = (value != DETACHED_HEAD).then(|| value.to_owned()),
        BRANCH_UPSTREAM => status.upstream = Some(value.to_owned()),
        BRANCH_AHEAD_BEHIND => {
            for count in value.split(FIELD_SEPARATOR) {
                if let Some(ahead) = count.strip_prefix(AHEAD_PREFIX) {
                    status.ahead = ahead.parse().unwrap_or_default();
                } else if let Some(behind) = count.strip_prefix(BEHIND_PREFIX) {
                    status.behind = behind.parse().unwrap_or_default();
                }
            }
        }
        _ => {}
    }
}

fn push_change(
    status: &mut GitStatus,
    record: &str,
    field_count: usize,
    kind_of: impl Fn(&str) -> ChangeKind,
) {
    let fields: Vec<&str> = record.splitn(field_count, FIELD_SEPARATOR).collect();
    if fields.len() != field_count {
        return;
    }
    let (Some(xy), Some(path)) = (fields.get(1), fields.last()) else {
        return;
    };
    status.changes.push(FileChange {
        path: PathBuf::from(path),
        kind: kind_of(xy),
    });
}

fn change_kind(xy: &str) -> ChangeKind {
    if xy.contains(ADDED_CODE) {
        ChangeKind::Added
    } else if xy.contains(DELETED_CODE) {
        ChangeKind::Deleted
    } else if xy.contains(RENAMED_CODE) {
        ChangeKind::Renamed
    } else {
        ChangeKind::Modified
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(path: &str, kind: ChangeKind) -> FileChange {
        FileChange {
            path: PathBuf::from(path),
            kind,
        }
    }

    #[test]
    fn parse_status_reads_headers_and_every_entry_kind() {
        let output = [
            "# branch.oid 1a2b3c4d5e6f708192a3b4c5d6e7f8091a2b3c4d",
            "# branch.head main",
            "# branch.upstream origin/main",
            "# branch.ab +2 -1",
            "1 .M N... 100644 100644 100644 aaaa bbbb src/main.rs",
            "1 A. N... 000000 100644 100644 0000 cccc docs/new file.md",
            "2 R. N... 100644 100644 100644 dddd dddd R100 src/renamed.rs",
            "src/original.rs",
            "u UU N... 100644 100644 100644 100644 eeee ffff 1111 Cargo.toml",
            "? notes/todo.txt",
            "! target/debug",
            "",
        ]
        .join("\0");

        let status = parse_status(output.as_bytes());

        assert_eq!(
            status,
            GitStatus {
                branch: Some("main".to_owned()),
                upstream: Some("origin/main".to_owned()),
                ahead: 2,
                behind: 1,
                changes: vec![
                    change("src/main.rs", ChangeKind::Modified),
                    change("docs/new file.md", ChangeKind::Added),
                    change("src/renamed.rs", ChangeKind::Renamed),
                    change("Cargo.toml", ChangeKind::Conflicted),
                    change("notes/todo.txt", ChangeKind::Untracked),
                ],
            }
        );
    }

    #[test]
    fn parse_status_skips_truncated_records() {
        let output = "1 .M N... 100644\0u UU N...\0";

        let status = parse_status(output.as_bytes());

        assert_eq!(status.changes, Vec::new());
    }

    #[test]
    fn parse_status_reports_detached_head_without_branch() {
        let output = "# branch.oid 1a2b3c\0# branch.head (detached)\0";

        let status = parse_status(output.as_bytes());

        assert_eq!(status.branch, None);
        assert_eq!(status.upstream, None);
    }
}
