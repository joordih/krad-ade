use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use krad_domain::{
    is_env_file, BaseFetch, EnvFiles, WorktreeBase, WorktreeFolder, WorktreeSettings,
};
use krad_host::{FileMode, HostLink};

use super::{worktree_path, Worktree, START_POINT};
use crate::git::run_git;
use crate::network::{fetch, Askpass};
use crate::process::git_path;
use crate::RepoError;

const DEFAULT_BRANCH_ARGS: [&str; 4] = [
    "symbolic-ref",
    "--quiet",
    "--short",
    "refs/remotes/origin/HEAD",
];

#[derive(Debug)]
pub struct PlannedWorktree {
    pub worktree: Worktree,
    pub setbacks: Vec<RepoError>,
}

#[must_use]
pub fn planned_worktree_path(repo_root: &Path, folder: &WorktreeFolder, name: &str) -> PathBuf {
    match folder {
        WorktreeFolder::BesideRepository => worktree_path(repo_root, name),
        WorktreeFolder::Custom(dir) => dir
            .join(repo_root.file_name().unwrap_or_default())
            .join(name),
    }
}

/// # Errors
///
/// Returns [`RepoError::Git`] when git cannot add the worktree. A failed fetch,
/// a missing default branch or an env file that could not be copied do not stop
/// the worktree; they come back in [`PlannedWorktree::setbacks`].
pub fn add_planned_worktree(
    host: &HostLink,
    repo_root: &Path,
    name: &str,
    settings: &WorktreeSettings,
    askpass: &Askpass,
) -> Result<PlannedWorktree, RepoError> {
    let mut setbacks = Vec::new();
    if settings.fetch == BaseFetch::FetchFirst {
        if let Err(error) = fetch(host, repo_root, askpass) {
            setbacks.push(error);
        }
    }
    let start = match settings.base {
        WorktreeBase::CurrentHead => START_POINT.to_owned(),
        WorktreeBase::DefaultBranch => default_branch(host, repo_root).unwrap_or_else(|error| {
            setbacks.push(error);
            START_POINT.to_owned()
        }),
    };
    let branch = settings.branch_prefix.branch(name);
    let path = planned_worktree_path(repo_root, &settings.folder, name);
    let target = git_path(host, repo_root, &path);
    let args: [&OsStr; 7] = [
        OsStr::new("worktree"),
        OsStr::new("add"),
        OsStr::new("--no-track"),
        OsStr::new("-b"),
        OsStr::new(&branch),
        &target,
        OsStr::new(&start),
    ];
    run_git(host, repo_root, &args)?;
    if settings.env_files == EnvFiles::Copy {
        setbacks.extend(copy_env_files(host, repo_root, &path));
    }
    Ok(PlannedWorktree {
        worktree: Worktree {
            path,
            branch: Some(branch),
        },
        setbacks,
    })
}

pub(crate) fn default_branch(host: &HostLink, repo_root: &Path) -> Result<String, RepoError> {
    let printed = run_git(host, repo_root, &DEFAULT_BRANCH_ARGS)?;
    let name = String::from_utf8_lossy(&printed).trim().to_owned();
    if name.is_empty() {
        return Err(RepoError::UnexpectedOutput(DEFAULT_BRANCH_ARGS.join(" ")));
    }
    Ok(name)
}

fn copy_env_files(host: &HostLink, repo_root: &Path, target: &Path) -> Vec<RepoError> {
    let entries = match host.read_dir(repo_root) {
        Ok(entries) => entries,
        Err(error) => return vec![error.into()],
    };
    entries
        .into_iter()
        .filter(|entry| is_env_file(&entry.name) && !entry.symlink && !entry.metadata.is_dir())
        .filter_map(|entry| {
            let copy = target.join(&entry.name);
            if host.exists(&copy) {
                return None;
            }
            host.read_all(&entry.path)
                .and_then(|bytes| host.write_with_mode(&copy, &bytes, FileMode::PRIVATE_FILE))
                .err()
                .map(RepoError::from)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_custom_folder_keeps_each_repository_apart() {
        let custom = WorktreeFolder::Custom(PathBuf::from("/trees"));
        assert_eq!(
            planned_worktree_path(Path::new("/src/app"), &custom, "fix"),
            PathBuf::from("/trees/app/fix")
        );
        assert_eq!(
            planned_worktree_path(
                Path::new("/src/app"),
                &WorktreeFolder::BesideRepository,
                "fix"
            ),
            PathBuf::from("/src/app.worktrees/fix")
        );
    }
}
