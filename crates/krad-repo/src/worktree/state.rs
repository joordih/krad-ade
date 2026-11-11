use std::ffi::OsStr;
use std::path::Path;

use krad_domain::{WorktreeBase, WorktreeCheck, WorktreeState};
use krad_host::HostLink;

use super::planned::default_branch;
use super::{COUNT_UNMERGED_ARGS, HEAD_ARGS};
use crate::git::run_git;
use crate::status::status;
use crate::RepoError;

const ARG_SEPARATOR: &str = " ";
const REFLOG_ARGS: [&str; 3] = ["reflog", "show", "--format=%H"];
const BRANCH_REF_PREFIX: &str = "refs/heads/";
const CREATION_ENTRIES: usize = 1;

/// # Errors
///
/// Returns [`RepoError::Git`] when git cannot read the worktree's status or
/// history, and [`RepoError::UnexpectedOutput`] when it prints something odd.
pub fn worktree_state(
    host: &HostLink,
    repo_root: &Path,
    path: &Path,
    branch: Option<&str>,
    base: WorktreeBase,
) -> Result<WorktreeState, RepoError> {
    let uncommitted = !status(host, path)?.changes.is_empty();
    if uncommitted {
        return Ok(WorktreeState::Dirty);
    }
    let base = base_commit(host, repo_root, base)?;
    let mut args: Vec<&OsStr> = COUNT_UNMERGED_ARGS.iter().map(OsStr::new).collect();
    args.push(OsStr::new(&base));
    let counted = String::from_utf8_lossy(&run_git(host, path, &args)?)
        .trim()
        .to_owned();
    let unmerged_commits: usize = counted
        .parse()
        .map_err(|_| RepoError::UnexpectedOutput(COUNT_UNMERGED_ARGS.join(ARG_SEPARATOR)))?;
    let committed = branch.is_some_and(|branch| has_moved(host, path, branch));
    Ok(WorktreeState::from(WorktreeCheck {
        uncommitted,
        unmerged_commits,
        committed,
    }))
}

fn base_commit(host: &HostLink, repo_root: &Path, base: WorktreeBase) -> Result<String, RepoError> {
    match base {
        WorktreeBase::DefaultBranch => {
            if let Ok(branch) = default_branch(host, repo_root) {
                return Ok(branch);
            }
            base_commit(host, repo_root, WorktreeBase::CurrentHead)
        }
        WorktreeBase::CurrentHead => Ok(String::from_utf8_lossy(&run_git(
            host, repo_root, &HEAD_ARGS,
        )?)
        .trim()
        .to_owned()),
    }
}

fn has_moved(host: &HostLink, path: &Path, branch: &str) -> bool {
    let reference = format!("{BRANCH_REF_PREFIX}{branch}");
    let mut args: Vec<&str> = REFLOG_ARGS.to_vec();
    args.push(&reference);
    run_git(host, path, &args).is_ok_and(|printed| reflog_moved(&String::from_utf8_lossy(&printed)))
}

fn reflog_moved(printed: &str) -> bool {
    printed
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count()
        > CREATION_ENTRIES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_branch_only_moved_once_it_has_more_than_its_creation_entry() {
        assert!(!reflog_moved(""));
        assert!(!reflog_moved("1a2b\n"));
        assert!(reflog_moved("3c4d\n1a2b\n"));
    }
}
