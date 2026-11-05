use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::process::{git_path, host_path};
use crate::refs::{Branch, BranchPlace};
use krad_host::HostLink;

use crate::git::run_git;
use crate::RepoError;

const WORKTREES_FOLDER_SUFFIX: &str = ".worktrees";
const START_POINT: &str = "HEAD";
const NAME_SEPARATOR: char = '-';
const ALLOWED_NAME_PUNCTUATION: [char; 3] = ['-', '_', '.'];
const EDGE_TRIMMED: [char; 2] = ['-', '.'];
const PARENT_DIR_MARKER: &str = "..";
const PRUNE_ARGS: [&str; 2] = ["worktree", "prune"];
const LIST_ARGS: [&str; 3] = ["worktree", "list", "--porcelain"];
const RECORD_SEPARATOR: &str = "\n\n";
const PATH_FIELD: &str = "worktree ";
const BRANCH_FIELD: &str = "branch ";
const BRANCH_REF_PREFIX: &str = "refs/heads/";
const BARE_FIELD: &str = "bare";
const PRUNABLE_FIELD: &str = "prunable";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,
    pub branch: Option<String>,
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

#[must_use]
pub fn worktree_path(repo_root: &Path, name: &str) -> PathBuf {
    let mut folder = repo_root.file_name().unwrap_or_default().to_owned();
    folder.push(WORKTREES_FOLDER_SUFFIX);
    repo_root
        .parent()
        .unwrap_or(repo_root)
        .join(folder)
        .join(name)
}

/// # Errors
pub fn add_worktree(host: &HostLink, repo_root: &Path, name: &str) -> Result<Worktree, RepoError> {
    let path = worktree_path(repo_root, name);
    let target = git_path(host, repo_root, &path);
    let args: [&OsStr; 7] = [
        OsStr::new("worktree"),
        OsStr::new("add"),
        OsStr::new("--no-track"),
        OsStr::new("-b"),
        OsStr::new(name),
        &target,
        OsStr::new(START_POINT),
    ];
    run_git(host, repo_root, &args)?;
    Ok(Worktree {
        path,
        branch: Some(name.to_owned()),
    })
}

/// # Errors
///
/// Returns [`RepoError::InvalidName`] when the branch gives no usable
/// folder name, and [`RepoError::Git`] when the branch is checked out
/// elsewhere or the folder exists.
pub fn add_branch_worktree(
    host: &HostLink,
    repo_root: &Path,
    branch: &Branch,
) -> Result<Worktree, RepoError> {
    let name = sanitize_worktree_name(&branch.name)
        .ok_or_else(|| RepoError::InvalidName(branch.name.clone()))?;
    let path = worktree_path(repo_root, &name);
    let target = git_path(host, repo_root, &path);
    let tracked = branch.to_string();
    let mut args: Vec<&OsStr> = vec![OsStr::new("worktree"), OsStr::new("add")];
    match &branch.place {
        BranchPlace::Local => args.extend([target.as_os_str(), OsStr::new(&branch.name)]),
        BranchPlace::Remote(_) => args.extend([
            OsStr::new("--track"),
            OsStr::new("-b"),
            OsStr::new(&branch.name),
            target.as_os_str(),
            OsStr::new(&tracked),
        ]),
    }
    run_git(host, repo_root, &args)?;
    Ok(Worktree {
        path,
        branch: Some(branch.name.clone()),
    })
}

/// # Errors
pub fn remove_worktree(host: &HostLink, repo_root: &Path, path: &Path) -> Result<(), RepoError> {
    let target = git_path(host, repo_root, path);
    let args: [&OsStr; 3] = [
        OsStr::new("worktree"),
        OsStr::new("remove"),
        target.as_os_str(),
    ];
    run_git(host, repo_root, &args)?;
    run_git(host, repo_root, &PRUNE_ARGS)?;
    Ok(())
}

/// # Errors
pub fn list_worktrees(host: &HostLink, repo: &Path) -> Result<Vec<Worktree>, RepoError> {
    let output = run_git(host, repo, &LIST_ARGS)?;
    Ok(parse_worktrees(
        &String::from_utf8_lossy(&output),
        |printed| host_path(host, repo, printed),
    ))
}

fn parse_worktrees(output: &str, path_of: impl Fn(&str) -> PathBuf) -> Vec<Worktree> {
    output
        .split(RECORD_SEPARATOR)
        .filter_map(|record| {
            let mut path = None;
            let mut branch = None;
            for line in record.lines() {
                if line == BARE_FIELD || line.starts_with(PRUNABLE_FIELD) {
                    return None;
                }
                if let Some(printed) = line.strip_prefix(PATH_FIELD) {
                    path = Some(path_of(printed));
                } else if let Some(name) = line.strip_prefix(BRANCH_FIELD) {
                    branch = Some(
                        name.strip_prefix(BRANCH_REF_PREFIX)
                            .unwrap_or(name)
                            .to_owned(),
                    );
                }
            }
            Some(Worktree {
                path: path?,
                branch,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn the_worktree_list_keeps_checked_out_folders_and_their_branches() {
        let output = "worktree /src/app\nHEAD 1a2b\nbranch refs/heads/main\n\n\
                      worktree /src/app.worktrees/fix\nHEAD 3c4d\ndetached\n\n\
                      worktree /src/bare\nbare\n\n\
                      worktree /gone\nHEAD 5e6f\nbranch refs/heads/old\nprunable gitdir file points to non-existent location\n";
        let listed = parse_worktrees(output, |printed| PathBuf::from(printed));
        assert_eq!(
            listed,
            [
                Worktree {
                    path: PathBuf::from("/src/app"),
                    branch: Some("main".to_owned()),
                },
                Worktree {
                    path: PathBuf::from("/src/app.worktrees/fix"),
                    branch: None,
                },
            ]
        );
    }

    #[test]
    fn worktree_path_is_a_sibling_folder_of_the_repository() {
        assert_eq!(
            worktree_path(Path::new("/src/app"), "fix-login"),
            PathBuf::from("/src/app.worktrees/fix-login")
        );
    }
}
