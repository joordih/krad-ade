use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use krad_domain::review::CommitSha;
use krad_domain::{CommitLabel, HistoryCommit};
use krad_host::HostLink;

use crate::changes::{parse_numstat, LineCount, WHOLE_FILE_CONTEXT};
use crate::git::run_git;
use crate::status::{ChangeKind, FileChange};
use crate::RepoError;

const FIELD: char = '\u{1f}';
const RECORD_SEPARATOR: u8 = 0;
const LOG_FORMAT: &str = "--format=%H%x1f%P%x1f%an%x1f%at%x1f%D%x1f%s";
const LOG_ARGS: [&str; 5] = ["log", "-z", "--topo-order", "--decorate=full", LOG_FORMAT];
const LIMIT_FLAG: &str = "--max-count=";
const PATHSPEC_SEPARATOR: &str = "--";
const LOG_FIELDS: usize = 6;
const PARENT_SEPARATOR: char = ' ';
const LABEL_SEPARATOR: &str = ", ";
const HEAD_POINTER: &str = "HEAD -> ";
const BARE_HEAD: &str = "HEAD";
const TAG_PREFIX: &str = "tag: refs/tags/";
const BRANCH_PREFIX: &str = "refs/heads/";
const REMOTE_PREFIX: &str = "refs/remotes/";
const REMOTE_HEAD_SUFFIX: &str = "/HEAD";
const TREE_ARGS: [&str; 7] = [
    "diff-tree",
    "-r",
    "-z",
    "--no-commit-id",
    "--root",
    "--find-renames",
    "--diff-merges=first-parent",
];
const NAME_STATUS: &str = "--name-status";
const NUMSTAT: &str = "--numstat";
const SHOW_ARGS: [&str; 7] = [
    "show",
    "--format=",
    "--no-color",
    "--no-ext-diff",
    "--find-renames",
    "--diff-merges=first-parent",
    WHOLE_FILE_CONTEXT,
];
const HUNK_PREFIX: &str = "@@";
const ADDED_CODE: char = 'A';
const DELETED_CODE: char = 'D';
const RENAMED_CODE: char = 'R';
const COPIED_CODE: char = 'C';
const UNMERGED_CODE: char = 'U';

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitFile {
    pub change: FileChange,
    pub lines: LineCount,
}

/// # Errors
///
/// Returns [`RepoError::Git`] when `git log` fails, as in a repository with
/// no commit yet.
pub fn commit_log(
    host: &HostLink,
    repo: &Path,
    revisions: &[&str],
    limit: usize,
) -> Result<Vec<HistoryCommit>, RepoError> {
    let limit = format!("{LIMIT_FLAG}{limit}");
    let mut args: Vec<&str> = LOG_ARGS.to_vec();
    args.push(&limit);
    args.extend(revisions);
    args.push(PATHSPEC_SEPARATOR);
    run_git(host, repo, &args).map(|stdout| parse_log(&stdout))
}

fn parse_log(output: &[u8]) -> Vec<HistoryCommit> {
    output
        .split(|&byte| byte == RECORD_SEPARATOR)
        .map(String::from_utf8_lossy)
        .filter_map(|record| parse_commit(record.trim_start_matches('\n')))
        .collect()
}

fn parse_commit(record: &str) -> Option<HistoryCommit> {
    let fields: Vec<&str> = record.splitn(LOG_FIELDS, FIELD).collect();
    let [sha, parents, author, time, labels, subject] = fields.as_slice() else {
        return None;
    };
    if sha.is_empty() {
        return None;
    }
    Some(HistoryCommit {
        sha: CommitSha((*sha).to_owned()),
        parents: parents
            .split(PARENT_SEPARATOR)
            .filter(|parent| !parent.is_empty())
            .map(|parent| CommitSha(parent.to_owned()))
            .collect(),
        author: (*author).to_owned(),
        time: SystemTime::UNIX_EPOCH + Duration::from_secs(time.parse().unwrap_or_default()),
        labels: parse_labels(labels),
        subject: (*subject).to_owned(),
    })
}

fn parse_labels(decoration: &str) -> Vec<CommitLabel> {
    decoration
        .split(LABEL_SEPARATOR)
        .filter_map(|label| {
            if let Some(branch) = label.strip_prefix(HEAD_POINTER) {
                let name = branch.strip_prefix(BRANCH_PREFIX).unwrap_or(branch);
                return Some(CommitLabel::Branch {
                    name: name.to_owned(),
                    current: true,
                });
            }
            if let Some(tag) = label.strip_prefix(TAG_PREFIX) {
                return Some(CommitLabel::Tag(tag.to_owned()));
            }
            if let Some(branch) = label.strip_prefix(BRANCH_PREFIX) {
                return Some(CommitLabel::Branch {
                    name: branch.to_owned(),
                    current: false,
                });
            }
            label
                .strip_prefix(REMOTE_PREFIX)
                .filter(|remote| !remote.ends_with(REMOTE_HEAD_SUFFIX))
                .map(|remote| CommitLabel::Remote(remote.to_owned()))
        })
        .filter(|label| !matches!(label, CommitLabel::Branch { name, .. } if name == BARE_HEAD))
        .collect()
}

/// # Errors
///
/// Returns [`RepoError::Git`] when the commit is not in the repository.
pub fn commit_files(
    host: &HostLink,
    repo: &Path,
    sha: &CommitSha,
) -> Result<Vec<CommitFile>, RepoError> {
    let tree = |flag: &'static str| {
        let mut args: Vec<&str> = TREE_ARGS.to_vec();
        args.extend([flag, sha.0.as_str()]);
        run_git(host, repo, &args)
    };
    let counts = parse_numstat(&tree(NUMSTAT)?);
    Ok(parse_name_status(&tree(NAME_STATUS)?)
        .into_iter()
        .map(|change| CommitFile {
            lines: counts.get(&change.path).copied().unwrap_or_default(),
            change,
        })
        .collect())
}

fn kind_of(code: &str) -> ChangeKind {
    match code.chars().next() {
        Some(ADDED_CODE | COPIED_CODE) => ChangeKind::Added,
        Some(DELETED_CODE) => ChangeKind::Deleted,
        Some(RENAMED_CODE) => ChangeKind::Renamed,
        Some(UNMERGED_CODE) => ChangeKind::Conflicted,
        _ => ChangeKind::Modified,
    }
}

fn parse_name_status(output: &[u8]) -> Vec<FileChange> {
    let mut fields = output
        .split(|&byte| byte == RECORD_SEPARATOR)
        .map(String::from_utf8_lossy)
        .filter(|field| !field.is_empty());
    let mut changes = Vec::new();
    while let Some(code) = fields.next() {
        let kind = kind_of(&code);
        let two_paths = code.starts_with([RENAMED_CODE, COPIED_CODE]);
        let origin = if two_paths {
            fields.next().map(|path| PathBuf::from(path.as_ref()))
        } else {
            None
        };
        let Some(path) = fields.next() else {
            break;
        };
        changes.push(FileChange {
            path: PathBuf::from(path.as_ref()),
            kind,
            origin: origin.filter(|_| kind == ChangeKind::Renamed),
        });
    }
    changes
}

/// # Errors
///
/// Returns [`RepoError::Git`] when the commit is not in the repository.
pub fn commit_diff(
    host: &HostLink,
    repo: &Path,
    sha: &CommitSha,
    change: &FileChange,
) -> Result<String, RepoError> {
    let mut args: Vec<&OsStr> = SHOW_ARGS.iter().map(OsStr::new).collect();
    args.extend([
        OsStr::new(sha.0.as_str()),
        OsStr::new(PATHSPEC_SEPARATOR),
        change.path.as_os_str(),
    ]);
    args.extend(change.origin.iter().map(|origin| origin.as_os_str()));
    let stdout = run_git(host, repo, &args)?;
    let diff = String::from_utf8_lossy(&stdout);
    Ok(diff
        .find(HUNK_PREFIX)
        .map_or_else(String::new, |start| diff[start..].to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_log_record_reads_parents_labels_and_subject() {
        let output = "aaa\u{1f}bbb ccc\u{1f}Ada\u{1f}60\u{1f}HEAD -> refs/heads/main, tag: refs/tags/v1, refs/remotes/origin/main, refs/remotes/origin/HEAD\u{1f}Merge it\0\nbbb\u{1f}\u{1f}Ada\u{1f}0\u{1f}\u{1f}Start\0";
        let commits = parse_log(output.as_bytes());
        assert_eq!(commits.len(), 2);
        assert_eq!(
            commits[0].parents,
            [CommitSha("bbb".to_owned()), CommitSha("ccc".to_owned())]
        );
        assert_eq!(
            commits[0].labels,
            [
                CommitLabel::Branch {
                    name: "main".to_owned(),
                    current: true
                },
                CommitLabel::Tag("v1".to_owned()),
                CommitLabel::Remote("origin/main".to_owned()),
            ]
        );
        assert_eq!(
            commits[0].time,
            SystemTime::UNIX_EPOCH + Duration::from_secs(60)
        );
        assert_eq!(commits[1].parents, []);
        assert_eq!(commits[1].labels, []);
        assert_eq!(commits[1].subject, "Start");
    }

    #[test]
    fn name_status_reads_plain_and_renamed_files() {
        let output = b"M\0src/lib.rs\0R087\0old.rs\0new.rs\0A\0docs/a.md\0";
        assert_eq!(
            parse_name_status(output),
            [
                FileChange {
                    path: PathBuf::from("src/lib.rs"),
                    kind: ChangeKind::Modified,
                    origin: None,
                },
                FileChange {
                    path: PathBuf::from("new.rs"),
                    kind: ChangeKind::Renamed,
                    origin: Some(PathBuf::from("old.rs")),
                },
                FileChange {
                    path: PathBuf::from("docs/a.md"),
                    kind: ChangeKind::Added,
                    origin: None,
                },
            ]
        );
    }
}
