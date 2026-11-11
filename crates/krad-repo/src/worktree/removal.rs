use std::ffi::OsStr;
use std::path::Path;

use krad_domain::{WorktreeBase, WorktreeState};
use krad_host::HostLink;

use super::state::worktree_state;
use super::PRUNE_ARGS;
use crate::git::run_git;
use crate::process::git_path;
use crate::RepoError;

const FORCE_FLAG: &str = "--force";

/// # Errors
///
/// Returns [`RepoError::Git`] when git cannot read or remove the worktree.
pub fn remove_merged_worktree(
    host: &HostLink,
    repo_root: &Path,
    path: &Path,
    branch: Option<&str>,
    base: WorktreeBase,
) -> Result<WorktreeState, RepoError> {
    let state = worktree_state(host, repo_root, path, branch, base)?;
    if state == WorktreeState::Merged {
        super::remove_worktree(host, repo_root, path)?;
    }
    Ok(state)
}

/// # Errors
///
/// Returns [`RepoError::Git`] when git cannot remove the worktree.
pub fn discard_worktree(host: &HostLink, repo_root: &Path, path: &Path) -> Result<(), RepoError> {
    let target = git_path(host, repo_root, path);
    let args: [&OsStr; 4] = [
        OsStr::new("worktree"),
        OsStr::new("remove"),
        OsStr::new(FORCE_FLAG),
        target.as_os_str(),
    ];
    run_git(host, repo_root, &args)?;
    run_git(host, repo_root, &PRUNE_ARGS)?;
    Ok(())
}
