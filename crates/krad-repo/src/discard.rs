use std::ffi::OsStr;
use std::path::Path;

use krad_host::HostLink;

use crate::file_ops::delete_entry;
use crate::git::run_git;
use crate::status::{ChangeKind, FileChange};
use crate::RepoError;

const PATHSPEC_SEPARATOR: &str = "--";
const RESTORE_ARGS: [&str; 4] = ["restore", "--source=HEAD", "--staged", "--worktree"];
const REMOVE_ARGS: [&str; 3] = ["rm", "--force", "--quiet"];

fn git_on_paths(
    host: &HostLink,
    repo: &Path,
    command: &[&str],
    paths: &[&Path],
) -> Result<(), RepoError> {
    let mut args: Vec<&OsStr> = command.iter().map(OsStr::new).collect();
    args.push(OsStr::new(PATHSPEC_SEPARATOR));
    args.extend(paths.iter().map(|path| path.as_os_str()));
    run_git(host, repo, &args).map(drop)
}

/// # Errors
///
/// Returns [`RepoError::Git`] when git cannot restore the file and
/// [`RepoError::Io`] when an untracked file cannot be removed.
pub fn discard_change(host: &HostLink, repo: &Path, change: &FileChange) -> Result<(), RepoError> {
    let path = change.path.as_path();
    match change.kind {
        ChangeKind::Untracked => delete_entry(host, &repo.join(path)),
        ChangeKind::Added => git_on_paths(host, repo, &REMOVE_ARGS, &[path]),
        ChangeKind::Renamed => {
            git_on_paths(host, repo, &REMOVE_ARGS, &[path])?;
            match change.origin.as_deref() {
                Some(origin) => git_on_paths(host, repo, &RESTORE_ARGS, &[origin]),
                None => Ok(()),
            }
        }
        ChangeKind::Modified | ChangeKind::Deleted | ChangeKind::Conflicted => {
            git_on_paths(host, repo, &RESTORE_ARGS, &[path])
        }
    }
}
