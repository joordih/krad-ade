use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::process::git_path;

use crate::git::run_git;
use crate::RepoError;

const WORKTREES_FOLDER_SUFFIX: &str = ".worktrees";
const START_POINT: &str = "HEAD";
const NAME_SEPARATOR: char = '-';
const ALLOWED_NAME_PUNCTUATION: [char; 3] = ['-', '_', '.'];
const EDGE_TRIMMED: [char; 2] = ['-', '.'];
const PARENT_DIR_MARKER: &str = "..";
const PRUNE_ARGS: [&str; 2] = ["worktree", "prune"];

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
pub fn add_worktree(repo_root: &Path, name: &str) -> Result<Worktree, RepoError> {
    let path = worktree_path(repo_root, name);
    let target = git_path(repo_root, &path);
    let args: [&OsStr; 7] = [
        OsStr::new("worktree"),
        OsStr::new("add"),
        OsStr::new("--no-track"),
        OsStr::new("-b"),
        OsStr::new(name),
        &target,
        OsStr::new(START_POINT),
    ];
    run_git(repo_root, &args)?;
    Ok(Worktree {
        path,
        branch: Some(name.to_owned()),
    })
}

/// # Errors
pub fn remove_worktree(repo_root: &Path, path: &Path) -> Result<(), RepoError> {
    let target = git_path(repo_root, path);
    let args: [&OsStr; 3] = [
        OsStr::new("worktree"),
        OsStr::new("remove"),
        target.as_os_str(),
    ];
    run_git(repo_root, &args)?;
    run_git(repo_root, &PRUNE_ARGS)?;
    Ok(())
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
    fn worktree_path_is_a_sibling_folder_of_the_repository() {
        assert_eq!(
            worktree_path(Path::new("/src/app"), "fix-login"),
            PathBuf::from("/src/app.worktrees/fix-login")
        );
    }
}
