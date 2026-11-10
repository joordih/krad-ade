use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use krad_domain::GitObjectId;
use krad_host::HostLink;

use crate::git::{run_git, run_git_within};
use crate::process::host_path;
use crate::status::{status, GitStatus};
use crate::RepoError;

const LOCATION_ARGS: [&str; 3] = ["rev-parse", "--show-toplevel", "--absolute-git-dir"];
const HAS_HEAD_ARGS: [&str; 3] = ["rev-parse", "--verify", "--quiet"];
const HEAD_TREE: &str = "HEAD^{tree}";
const UPDATE_INDEX_ARGS: [&str; 4] = ["update-index", "--add", "--remove", "--"];
const READ_HEAD_ARGS: [&str; 2] = ["read-tree", "HEAD"];
const WRITE_TREE_ARGS: [&str; 1] = ["write-tree"];
const DIFF_TREE_ARGS: [&str; 5] = ["diff-tree", "-r", "-z", "--no-renames", "--name-status"];
const RESTORE_ARGS: [&str; 2] = ["restore", "--worktree"];
const SOURCE_FLAG: &str = "--source=";
const PATHSPEC_SEPARATOR: &str = "--";
const TOP_LITERAL_PATHSPEC: &str = ":(top,literal)";
const INDEX_FILE_VAR: &str = "GIT_INDEX_FILE";
const SCRATCH_INDEX_PREFIX: &str = "krad-checkpoint-index";
const SCRATCH_INDEX_SEPARATOR: &str = "-";
const PRINTED_SEPARATOR: char = '/';
const PRINTED_SEPARATORS: [char; 2] = [PRINTED_SEPARATOR, '\\'];
const ARG_SEPARATOR: &str = " ";
const RECORD_SEPARATOR: u8 = 0;
const NEWLINE: char = '\n';
const STATUS_ADDED: &str = "A";
const STATUS_DELETED: &str = "D";
const PATHS_PER_COMMAND: usize = 256;
const CHECKPOINT_WAIT: Duration = Duration::from_mins(2);

static SCRATCH_INDEX_COUNTER: AtomicU64 = AtomicU64::new(0);

fn parse_checkpoint(printed: &[u8]) -> Result<GitObjectId, RepoError> {
    String::from_utf8_lossy(printed)
        .parse()
        .map_err(|_| RepoError::UnexpectedOutput(WRITE_TREE_ARGS.join(ARG_SEPARATOR)))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CheckpointChangeKind {
    Reverted,
    Recreated,
    Removed,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CheckpointChange {
    pub path: PathBuf,
    pub kind: CheckpointChangeKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[must_use]
pub struct RestoredCheckpoint {
    pub undo: GitObjectId,
    pub changes: Vec<CheckpointChange>,
}

struct Location {
    top: PathBuf,
    git_dir_printed: String,
    git_dir: PathBuf,
}

fn locate(host: &HostLink, repo: &Path) -> Result<Location, RepoError> {
    let printed = run_git(host, repo, &LOCATION_ARGS)?;
    let printed = String::from_utf8_lossy(&printed);
    let mut lines = printed.split(NEWLINE).map(str::trim_end);
    let (Some(top), Some(git_dir)) = (lines.next(), lines.next()) else {
        return Err(RepoError::UnexpectedOutput(
            LOCATION_ARGS.join(ARG_SEPARATOR),
        ));
    };
    Ok(Location {
        top: host_path(host, repo, top),
        git_dir_printed: git_dir.to_owned(),
        git_dir: host_path(host, repo, git_dir),
    })
}

fn has_head(host: &HostLink, repo: &Path) -> bool {
    let mut args: Vec<&str> = HAS_HEAD_ARGS.to_vec();
    args.push(HEAD_TREE);
    run_git(host, repo, &args).is_ok()
}

fn scratch_index_name() -> String {
    let count = SCRATCH_INDEX_COUNTER.fetch_add(1, Ordering::Relaxed);
    [
        SCRATCH_INDEX_PREFIX.to_owned(),
        std::process::id().to_string(),
        count.to_string(),
    ]
    .join(SCRATCH_INDEX_SEPARATOR)
}

fn printed_join(dir: &str, name: &str) -> String {
    if dir.ends_with(PRINTED_SEPARATORS) {
        return format!("{dir}{name}");
    }
    format!("{dir}{PRINTED_SEPARATOR}{name}")
}

/// # Errors
///
/// Returns [`RepoError::Git`] when `repo` is not a git repository or git
/// cannot hash its files, and [`RepoError::Io`] when the scratch index cannot
/// be removed.
pub fn snapshot_checkpoint(host: &HostLink, repo: &Path) -> Result<GitObjectId, RepoError> {
    let location = locate(host, repo)?;
    snapshot_at(host, repo, &location)
}

fn snapshot_at(
    host: &HostLink,
    repo: &Path,
    location: &Location,
) -> Result<GitObjectId, RepoError> {
    let name = scratch_index_name();
    let scratch = location.git_dir.join(&name);
    let env = [(
        INDEX_FILE_VAR.to_owned(),
        OsString::from(printed_join(&location.git_dir_printed, &name)),
    )];
    let written = write_tree(host, repo, location, &env);
    let removed = match host.remove_file(&scratch) {
        Err(error) if !error.is_not_found() => Err(RepoError::from(error)),
        _ => Ok(()),
    };
    let checkpoint = written?;
    removed?;
    Ok(checkpoint)
}

fn write_tree(
    host: &HostLink,
    repo: &Path,
    location: &Location,
    env: &[(String, OsString)],
) -> Result<GitObjectId, RepoError> {
    let status = status(host, repo)?;
    let changed = changed_paths(&status);
    if has_head(host, repo) {
        run_git_within(host, repo, &READ_HEAD_ARGS, env, CHECKPOINT_WAIT)?;
    }
    for chunk in changed.chunks(PATHS_PER_COMMAND) {
        let args: Vec<&OsStr> = UPDATE_INDEX_ARGS
            .iter()
            .map(OsStr::new)
            .chain(chunk.iter().map(|path| path.as_os_str()))
            .collect();
        run_git_within(host, &location.top, &args, env, CHECKPOINT_WAIT)?;
    }
    parse_checkpoint(&run_git_within(
        host,
        repo,
        &WRITE_TREE_ARGS,
        env,
        CHECKPOINT_WAIT,
    )?)
}

fn changed_paths(status: &GitStatus) -> Vec<&Path> {
    let mut paths: Vec<&Path> = status
        .changes
        .iter()
        .flat_map(|change| std::iter::once(&change.path).chain(change.origin.as_ref()))
        .map(PathBuf::as_path)
        .collect();
    paths.sort_unstable();
    paths.dedup();
    paths
}

/// # Errors
///
/// As [`snapshot_checkpoint`], and [`RepoError::Git`] when `checkpoint` is
/// not an object of this repository.
pub fn checkpoint_changes(
    host: &HostLink,
    repo: &Path,
    checkpoint: &GitObjectId,
) -> Result<Vec<CheckpointChange>, RepoError> {
    let current = snapshot_checkpoint(host, repo)?;
    changes_between(host, repo, &current, checkpoint)
}

fn changes_between(
    host: &HostLink,
    repo: &Path,
    from: &GitObjectId,
    to: &GitObjectId,
) -> Result<Vec<CheckpointChange>, RepoError> {
    let mut args: Vec<&str> = DIFF_TREE_ARGS.to_vec();
    args.extend([from.as_str(), to.as_str()]);
    Ok(parse_name_status(&run_git(host, repo, &args)?))
}

fn parse_name_status(output: &[u8]) -> Vec<CheckpointChange> {
    let mut fields = output
        .split(|&byte| byte == RECORD_SEPARATOR)
        .map(String::from_utf8_lossy);
    let mut changes = Vec::new();
    while let (Some(status), Some(path)) = (fields.next(), fields.next()) {
        let kind = match status.as_ref() {
            STATUS_ADDED => CheckpointChangeKind::Recreated,
            STATUS_DELETED => CheckpointChangeKind::Removed,
            _ => CheckpointChangeKind::Reverted,
        };
        changes.push(CheckpointChange {
            path: PathBuf::from(path.as_ref()),
            kind,
        });
    }
    changes
}

/// # Errors
///
/// As [`checkpoint_changes`], [`RepoError::Git`] when git cannot write a
/// file back and [`RepoError::Io`] when a file cannot be removed. The undo
/// checkpoint is taken before any file changes, so a failure part way leaves
/// the user's work recoverable from the files still in place.
pub fn restore_checkpoint(
    host: &HostLink,
    repo: &Path,
    checkpoint: &GitObjectId,
) -> Result<RestoredCheckpoint, RepoError> {
    let location = locate(host, repo)?;
    let undo = snapshot_at(host, repo, &location)?;
    let changes = changes_between(host, repo, &undo, checkpoint)?;
    let (removed, written): (Vec<&CheckpointChange>, Vec<&CheckpointChange>) = changes
        .iter()
        .partition(|change| change.kind == CheckpointChangeKind::Removed);
    for chunk in written.chunks(PATHS_PER_COMMAND) {
        write_back(host, repo, checkpoint, chunk)?;
    }
    for change in removed {
        remove_file(host, &location.top.join(&change.path))?;
    }
    Ok(RestoredCheckpoint { undo, changes })
}

fn write_back(
    host: &HostLink,
    repo: &Path,
    checkpoint: &GitObjectId,
    changes: &[&CheckpointChange],
) -> Result<(), RepoError> {
    let source = format!("{SOURCE_FLAG}{}", checkpoint.as_str());
    let mut args: Vec<OsString> = RESTORE_ARGS.iter().map(OsString::from).collect();
    args.push(OsString::from(source));
    args.push(OsString::from(PATHSPEC_SEPARATOR));
    args.extend(changes.iter().map(|change| {
        let mut pathspec = OsString::from(TOP_LITERAL_PATHSPEC);
        pathspec.push(OsStr::new(&change.path));
        pathspec
    }));
    run_git(host, repo, &args).map(drop)
}

fn remove_file(host: &HostLink, path: &Path) -> Result<(), RepoError> {
    match host.remove_file(path) {
        Err(error) if !error.is_not_found() => Err(error.into()),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_status_reads_which_files_a_restore_writes_back_or_removes() {
        let output = b"M\0src/main.rs\0A\0gone.txt\0D\0new dir/added.txt\0T\0link\0";
        assert_eq!(
            parse_name_status(output),
            vec![
                CheckpointChange {
                    path: PathBuf::from("src/main.rs"),
                    kind: CheckpointChangeKind::Reverted,
                },
                CheckpointChange {
                    path: PathBuf::from("gone.txt"),
                    kind: CheckpointChangeKind::Recreated,
                },
                CheckpointChange {
                    path: PathBuf::from("new dir/added.txt"),
                    kind: CheckpointChangeKind::Removed,
                },
                CheckpointChange {
                    path: PathBuf::from("link"),
                    kind: CheckpointChangeKind::Reverted,
                },
            ]
        );
        assert_eq!(parse_name_status(b""), Vec::new());
    }

    #[test]
    fn a_checkpoint_is_only_a_printed_object_id() {
        assert_eq!(
            parse_checkpoint(b"4b825dc642cb6eb9a060e54bf8d69288fbee4904\n")
                .map(String::from)
                .ok()
                .as_deref(),
            Some("4b825dc642cb6eb9a060e54bf8d69288fbee4904")
        );
        assert!(parse_checkpoint(b"").is_err());
        assert!(parse_checkpoint(b"fatal: no").is_err());
    }
}
