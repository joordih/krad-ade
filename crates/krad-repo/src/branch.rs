use krad_domain::review::resolve_ssh_alias;
use std::path::{Component, Path};

use crate::files::{decode_text, sort_entries};
use crate::git::run_git;
use crate::{DirEntry, EntryKind, RepoError, TextFile};

const FETCH_ARGS: [&str; 3] = ["fetch", "--all", "--prune"];
const BRANCH_LIST_ARGS: [&str; 4] = [
    "for-each-ref",
    "--format=%(refname:short)%00%(symref)",
    "refs/heads",
    "refs/remotes",
];
const REF_FIELD_SEPARATOR: char = '\0';
const SSH_DIR: &str = ".ssh";
const SSH_CONFIG: &str = "config";
const TREE_RECORD_SEPARATOR: u8 = 0;
const TREE_PATH_SEPARATOR: char = '\t';
const TREE_FIELD_SEPARATOR: char = ' ';
const TREE_TYPE: &str = "tree";
const GIT_PATH_SEPARATOR: &str = "/";
const REVISION_PATH_SEPARATOR: &str = ":";
const NO_SUCH_REMOTE: &str = "No such remote";
const DIFF_ARGS: [&str; 4] = ["diff", "--no-color", "--no-ext-diff", "--find-renames"];
const PATHSPEC_SEPARATOR: &str = "--";
const HUNK_PREFIX: &str = "@@";

pub const ORIGIN_REMOTE: &str = "origin";

/// # Errors
///
/// Returns [`RepoError::Git`] when `git fetch` fails, as when offline or not
/// signed in to the remote.
pub fn fetch_branches(repo: &Path) -> Result<(), RepoError> {
    run_git(repo, &FETCH_ARGS).map(drop)
}

/// # Errors
///
/// Returns [`RepoError::Git`] when `repo` is not a git repository.
pub fn remote_url(repo: &Path, remote: &str) -> Result<Option<String>, RepoError> {
    match run_git(repo, &["remote", "get-url", remote]) {
        Ok(stdout) => Ok(Some(String::from_utf8_lossy(&stdout).trim().to_owned())),
        Err(RepoError::Git { message, .. }) if message.contains(NO_SUCH_REMOTE) => Ok(None),
        Err(error) => Err(error),
    }
}

/// # Errors
///
/// As [`remote_url`].
pub fn resolved_remote_url(
    repo: &Path,
    remote: &str,
    home: Option<&Path>,
) -> Result<Option<String>, RepoError> {
    let Some(url) = remote_url(repo, remote)? else {
        return Ok(None);
    };
    let config = home
        .and_then(|home| std::fs::read_to_string(home.join(SSH_DIR).join(SSH_CONFIG)).ok())
        .unwrap_or_default();
    Ok(Some(resolve_ssh_alias(&url, &config).into_owned()))
}

/// # Errors
///
/// Returns [`RepoError::Git`] when `repo` is not a git repository.
pub fn list_branches(repo: &Path) -> Result<Vec<String>, RepoError> {
    let stdout = run_git(repo, &BRANCH_LIST_ARGS)?;
    Ok(String::from_utf8_lossy(&stdout)
        .lines()
        .filter_map(|line| match line.split_once(REF_FIELD_SEPARATOR) {
            Some((name, "")) => Some(name.to_owned()),
            Some(_) => None,
            None => Some(line.to_owned()),
        })
        .collect())
}

/// # Errors
///
/// Returns [`RepoError::OutsideRepository`] when `dir` is not inside
/// `repo_root`, and [`RepoError::Git`] when `branch` does not exist.
pub fn list_tree(repo_root: &Path, branch: &str, dir: &Path) -> Result<Vec<DirEntry>, RepoError> {
    let relative = git_relative_path(repo_root, dir)?;
    let mut args = vec!["ls-tree", "-z", branch, "--"];
    let folder = format!("{relative}{GIT_PATH_SEPARATOR}");
    if !relative.is_empty() {
        args.push(&folder);
    }
    let stdout = run_git(repo_root, &args)?;
    let mut entries: Vec<DirEntry> = stdout
        .split(|&byte| byte == TREE_RECORD_SEPARATOR)
        .filter_map(|record| tree_entry(repo_root, &String::from_utf8_lossy(record)))
        .collect();
    sort_entries(&mut entries);
    Ok(entries)
}

fn tree_entry(repo_root: &Path, record: &str) -> Option<DirEntry> {
    let (fields, path) = record.split_once(TREE_PATH_SEPARATOR)?;
    let object_type = fields.split(TREE_FIELD_SEPARATOR).nth(1)?;
    let path = path
        .split(GIT_PATH_SEPARATOR)
        .fold(repo_root.to_path_buf(), |path, part| path.join(part));
    Some(DirEntry {
        name: path.file_name()?.to_string_lossy().into_owned(),
        path,
        kind: if object_type == TREE_TYPE {
            EntryKind::Directory
        } else {
            EntryKind::File
        },
    })
}

/// # Errors
///
/// Returns [`RepoError::Binary`] for a binary file,
/// [`RepoError::OutsideRepository`] when `path` is not inside `repo_root` and
/// [`RepoError::Git`] when `branch` lacks the file.
pub fn read_text_at(
    repo_root: &Path,
    branch: &str,
    path: &Path,
    max_bytes: u64,
) -> Result<TextFile, RepoError> {
    let relative = git_relative_path(repo_root, path)?;
    let revision = format!("{branch}{REVISION_PATH_SEPARATOR}{relative}");
    decode_text(run_git(repo_root, &["show", &revision])?, max_bytes)
}

/// # Errors
///
/// Returns [`RepoError::Git`] when either commit is not in the local
/// repository, as before fetching a branch pushed from elsewhere.
pub fn diff_between(
    repo: &Path,
    base: &str,
    head: &str,
    paths: &[&str],
) -> Result<String, RepoError> {
    let mut args: Vec<&str> = DIFF_ARGS.to_vec();
    args.extend([base, head, PATHSPEC_SEPARATOR]);
    args.extend(paths);
    let stdout = run_git(repo, &args)?;
    Ok(hunks_only(&String::from_utf8_lossy(&stdout)))
}

fn hunks_only(diff: &str) -> String {
    diff.find(HUNK_PREFIX)
        .map_or_else(String::new, |start| diff[start..].to_owned())
}

/// # Errors
///
/// Returns [`RepoError::OutsideRepository`] when `path` is not inside
/// `repo_root`.
pub fn git_relative_path(repo_root: &Path, path: &Path) -> Result<String, RepoError> {
    let outside = || RepoError::OutsideRepository(path.display().to_string());
    let relative = path.strip_prefix(repo_root).map_err(|_| outside())?;
    relative
        .components()
        .map(|part| match part {
            Component::Normal(name) => Ok(name.to_string_lossy()),
            _ => Err(outside()),
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|parts| parts.join(GIT_PATH_SEPARATOR))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn git(repo: &Path, args: &[&str]) -> Result<(), Box<dyn Error>> {
        let status = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
            .args(args)
            .output()?
            .status;
        if status.success() {
            Ok(())
        } else {
            Err(format!("git {args:?} failed").into())
        }
    }

    fn repo_with_feature_branch(label: &str) -> Result<PathBuf, Box<dyn Error>> {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let repo = std::env::temp_dir().join(format!(
            "krad-repo-branch-{label}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(repo.join("src"))?;
        git(&repo, &["init", "-q", "-b", "main"])?;
        fs::write(repo.join("src").join("lib.rs"), "main\n")?;
        fs::write(repo.join("README.md"), "readme\n")?;
        git(&repo, &["add", "."])?;
        git(&repo, &["commit", "-q", "-m", "main"])?;
        git(&repo, &["checkout", "-q", "-b", "feature"])?;
        fs::create_dir_all(repo.join("docs"))?;
        fs::write(repo.join("docs").join("notes.md"), "notes\n")?;
        fs::write(repo.join("src").join("lib.rs"), "feature\n")?;
        git(&repo, &["add", "."])?;
        git(&repo, &["commit", "-q", "-m", "feature"])?;
        git(&repo, &["checkout", "-q", "main"])?;
        Ok(repo)
    }

    fn names(entries: &[DirEntry]) -> Vec<(&str, EntryKind)> {
        entries
            .iter()
            .map(|entry| (entry.name.as_str(), entry.kind))
            .collect()
    }

    #[test]
    fn branches_list_locals_then_remotes_without_head_aliases() -> Result<(), Box<dyn Error>> {
        let repo = repo_with_feature_branch("list")?;
        git(&repo, &["update-ref", "refs/remotes/origin/main", "main"])?;
        git(
            &repo,
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/main",
            ],
        )?;

        let branches = list_branches(&repo);
        fs::remove_dir_all(&repo)?;

        assert_eq!(branches?, ["feature", "main", "origin/main"]);
        Ok(())
    }

    #[test]
    fn another_branch_lists_its_own_folders_first() -> Result<(), Box<dyn Error>> {
        let repo = repo_with_feature_branch("tree")?;

        let root = list_tree(&repo, "feature", &repo);
        let src = list_tree(&repo, "feature", &repo.join("src"));
        let main_root = list_tree(&repo, "main", &repo);
        fs::remove_dir_all(&repo)?;

        let root = root?;
        assert_eq!(
            names(&root),
            [
                ("docs", EntryKind::Directory),
                ("src", EntryKind::Directory),
                ("README.md", EntryKind::File)
            ]
        );
        assert_eq!(root[0].path, repo.join("docs"));
        let src = src?;
        assert_eq!(names(&src), [("lib.rs", EntryKind::File)]);
        assert_eq!(src[0].path, repo.join("src").join("lib.rs"));
        assert_eq!(
            names(&main_root?),
            [
                ("src", EntryKind::Directory),
                ("README.md", EntryKind::File)
            ]
        );
        Ok(())
    }

    #[test]
    fn files_read_as_the_branch_holds_them() -> Result<(), Box<dyn Error>> {
        let repo = repo_with_feature_branch("show")?;
        let lib = repo.join("src").join("lib.rs");

        let feature = read_text_at(&repo, "feature", &lib, 100);
        let main = read_text_at(&repo, "main", &lib, 100);
        let missing = read_text_at(&repo, "main", &repo.join("docs").join("notes.md"), 100);
        let outside = read_text_at(&repo, "main", Path::new("/elsewhere/lib.rs"), 100);
        fs::remove_dir_all(&repo)?;

        assert_eq!(feature?.text, "feature\n");
        assert_eq!(main?.text, "main\n");
        assert!(matches!(missing, Err(RepoError::Git { .. })));
        assert!(matches!(outside, Err(RepoError::OutsideRepository(_))));
        Ok(())
    }

    #[test]
    fn a_local_diff_holds_only_the_hunks_of_the_asked_paths() -> Result<(), Box<dyn Error>> {
        let repo = repo_with_feature_branch("diff")?;

        let diff = diff_between(&repo, "main", "feature", &["src/lib.rs"]);
        let missing = diff_between(&repo, "main", "0123456789abcdef", &["src/lib.rs"]);
        fs::remove_dir_all(&repo)?;

        assert_eq!(diff?, "@@ -1 +1 @@\n-main\n+feature\n");
        assert!(matches!(missing, Err(RepoError::Git { .. })));
        Ok(())
    }

    #[test]
    fn the_origin_url_is_read_and_a_missing_remote_is_none() -> Result<(), Box<dyn Error>> {
        let repo = repo_with_feature_branch("remote")?;
        let missing = remote_url(&repo, ORIGIN_REMOTE);
        git(
            &repo,
            &[
                "remote",
                "add",
                ORIGIN_REMOTE,
                "git@gitlab.example.test:team/app.git",
            ],
        )?;
        let origin = remote_url(&repo, ORIGIN_REMOTE);
        fs::remove_dir_all(&repo)?;

        assert_eq!(missing?, None);
        assert_eq!(
            origin?.as_deref(),
            Some("git@gitlab.example.test:team/app.git")
        );
        Ok(())
    }
}
