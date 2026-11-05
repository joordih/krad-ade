use krad_host::HostLink;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use crate::git::run_git;
use crate::process::git_path;
use crate::RepoError;

const PATHSPEC_SEPARATOR: &str = "--";
const STAGE_ARGS: [&str; 2] = ["add", "--all"];
const UNSTAGE_ARGS: [&str; 2] = ["restore", "--staged"];
const UNSTAGE_UNBORN_ARGS: [&str; 4] = ["rm", "--cached", "-r", "--quiet"];
const UNSTAGE_ALL_ARGS: [&str; 2] = ["reset", "--quiet"];
const WHOLE_TREE: &str = ".";
const COMMIT: &str = "commit";
const MESSAGE_FLAG: &str = "--message";
const AMEND_FLAG: &str = "--amend";
const NO_EDIT_FLAG: &str = "--no-edit";
const STASH_ARGS: [&str; 3] = ["stash", "push", "--include-untracked"];
const POP_STASH_ARGS: [&str; 2] = ["stash", "pop"];
const NO_HEAD_ARGS: [&str; 3] = ["rev-parse", "--verify", "HEAD"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitMode {
    New,
    Amend,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitScope {
    Staged,
    Everything,
}

fn with_paths(host: &HostLink, repo: &Path, args: &[&str], paths: &[PathBuf]) -> Vec<OsString> {
    args.iter()
        .copied()
        .chain(std::iter::once(PATHSPEC_SEPARATOR))
        .map(OsString::from)
        .chain(
            paths
                .iter()
                .map(|path| git_path(host, repo, &repo.join(path))),
        )
        .collect()
}

fn has_head(host: &HostLink, repo: &Path) -> bool {
    run_git(host, repo, &NO_HEAD_ARGS).is_ok()
}

/// # Errors
///
/// Returns [`RepoError::Git`] when a path is outside the repository.
pub fn stage(host: &HostLink, repo: &Path, paths: &[PathBuf]) -> Result<(), RepoError> {
    run_git(host, repo, &with_paths(host, repo, &STAGE_ARGS, paths)).map(drop)
}

/// # Errors
///
/// As [`stage`].
pub fn unstage(host: &HostLink, repo: &Path, paths: &[PathBuf]) -> Result<(), RepoError> {
    let args = if has_head(host, repo) {
        UNSTAGE_ARGS.as_slice()
    } else {
        UNSTAGE_UNBORN_ARGS.as_slice()
    };
    run_git(host, repo, &with_paths(host, repo, args, paths)).map(drop)
}

/// # Errors
///
/// Returns [`RepoError::Git`] when `repo` is not a git repository.
pub fn stage_all(host: &HostLink, repo: &Path) -> Result<(), RepoError> {
    run_git(host, repo, &STAGE_ARGS).map(drop)
}

/// # Errors
///
/// As [`stage_all`].
pub fn unstage_all(host: &HostLink, repo: &Path) -> Result<(), RepoError> {
    if has_head(host, repo) {
        return run_git(host, repo, &UNSTAGE_ALL_ARGS).map(drop);
    }
    let mut args: Vec<&str> = UNSTAGE_UNBORN_ARGS.to_vec();
    args.extend([PATHSPEC_SEPARATOR, WHOLE_TREE]);
    run_git(host, repo, &args).map(drop)
}

/// # Errors
///
/// Returns [`RepoError::Git`] when there is nothing to commit, the message
/// is empty or a hook refuses the commit.
pub fn commit(
    host: &HostLink,
    repo: &Path,
    message: &str,
    mode: CommitMode,
    scope: CommitScope,
) -> Result<(), RepoError> {
    if scope == CommitScope::Everything {
        stage_all(host, repo)?;
    }
    let mut args: Vec<&OsStr> = vec![OsStr::new(COMMIT)];
    if mode == CommitMode::Amend {
        args.push(OsStr::new(AMEND_FLAG));
    }
    if message.trim().is_empty() && mode == CommitMode::Amend {
        args.push(OsStr::new(NO_EDIT_FLAG));
    } else {
        args.extend([OsStr::new(MESSAGE_FLAG), OsStr::new(message)]);
    }
    run_git(host, repo, &args).map(drop)
}

/// # Errors
///
/// Returns [`RepoError::Git`] when there is nothing to set aside.
pub fn stash_changes(host: &HostLink, repo: &Path) -> Result<(), RepoError> {
    run_git(host, repo, &STASH_ARGS).map(drop)
}

/// # Errors
///
/// Returns [`RepoError::Git`] when there is no stash or it conflicts with
/// the working tree.
pub fn pop_stash(host: &HostLink, repo: &Path) -> Result<(), RepoError> {
    run_git(host, repo, &POP_STASH_ARGS).map(drop)
}
