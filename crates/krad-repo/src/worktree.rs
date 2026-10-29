use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::process::{git_path, host_path};

use crate::git::run_git;
use crate::RepoError;

const WORKTREES_FOLDER_SUFFIX: &str = ".worktrees";
const FIELD_SEPARATOR: u8 = 0;
const WORKTREE_FIELD: &str = "worktree ";
const BRANCH_FIELD: &str = "branch ";
const DETACHED_FIELD: &str = "detached";
const BARE_FIELD: &str = "bare";
const BRANCH_REF_PREFIX: &str = "refs/heads/";
const START_POINT: &str = "HEAD";
const NAME_SEPARATOR: char = '-';
const ALLOWED_NAME_PUNCTUATION: [char; 3] = ['-', '_', '.'];
const EDGE_TRIMMED: [char; 2] = ['-', '.'];
const PARENT_DIR_MARKER: &str = "..";
const LIST_ARGS: [&str; 4] = ["worktree", "list", "--porcelain", "-z"];
const PRUNE_ARGS: [&str; 2] = ["worktree", "prune"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub detached: bool,
    pub bare: bool,
}

impl Worktree {
    fn at(path: PathBuf) -> Self {
        Self {
            path,
            branch: None,
            detached: false,
            bare: false,
        }
    }
}

/// # Errors
pub fn list_worktrees(repo: &Path) -> Result<Vec<Worktree>, RepoError> {
    run_git(repo, &LIST_ARGS).map(|stdout| {
        parse_worktrees(&stdout)
            .into_iter()
            .map(|worktree| Worktree {
                path: host_path(repo, &worktree.path.to_string_lossy()),
                ..worktree
            })
            .collect()
    })
}

#[must_use]
pub fn parse_worktrees(output: &[u8]) -> Vec<Worktree> {
    let mut worktrees = Vec::new();
    let mut current: Option<Worktree> = None;
    for field in output.split(|&byte| byte == FIELD_SEPARATOR) {
        let field = String::from_utf8_lossy(field);
        if let Some(path) = field.strip_prefix(WORKTREE_FIELD) {
            worktrees.extend(current.replace(Worktree::at(PathBuf::from(path))));
            continue;
        }
        let Some(worktree) = current.as_mut() else {
            continue;
        };
        if field.is_empty() {
            worktrees.extend(current.take());
        } else if let Some(reference) = field.strip_prefix(BRANCH_FIELD) {
            let short = reference
                .strip_prefix(BRANCH_REF_PREFIX)
                .unwrap_or(reference);
            worktree.branch = Some(short.to_owned());
        } else if field == DETACHED_FIELD {
            worktree.detached = true;
        } else if field == BARE_FIELD {
            worktree.bare = true;
        }
    }
    worktrees.extend(current);
    worktrees
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
        branch: Some(name.to_owned()),
        ..Worktree::at(path)
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
    fn parse_worktrees_reads_main_linked_and_detached_records() {
        let output = [
            "worktree /src/app",
            "HEAD 1111111111111111111111111111111111111111",
            "branch refs/heads/main",
            "",
            "worktree /src/app.worktrees/fix-login",
            "HEAD 2222222222222222222222222222222222222222",
            "branch refs/heads/fix-login",
            "",
            "worktree /src/app.worktrees/review",
            "HEAD 3333333333333333333333333333333333333333",
            "detached",
            "",
            "",
        ]
        .join("\0");

        let worktrees = parse_worktrees(output.as_bytes());

        assert_eq!(
            worktrees,
            vec![
                Worktree {
                    branch: Some("main".to_owned()),
                    ..Worktree::at(PathBuf::from("/src/app"))
                },
                Worktree {
                    branch: Some("fix-login".to_owned()),
                    ..Worktree::at(PathBuf::from("/src/app.worktrees/fix-login"))
                },
                Worktree {
                    detached: true,
                    ..Worktree::at(PathBuf::from("/src/app.worktrees/review"))
                },
            ]
        );
    }

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
