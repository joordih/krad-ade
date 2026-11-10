use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};

use krad_host::HostLink;

use crate::file_ops::delete_entry;
use crate::git::{required_repo_root, run_git};
use crate::status::{status, ChangeKind, FileChange};
use crate::RepoError;

const PATHSPEC_SEPARATOR: &str = "--";
const RESTORE_ARGS: [&str; 4] = ["restore", "--source=HEAD", "--staged", "--worktree"];
const REMOVE_ARGS: [&str; 3] = ["rm", "--force", "--quiet"];
const LITERAL_PATHSPEC: &str = ":(literal)";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FolderChanges {
    pub tracked: Vec<PathBuf>,
    pub untracked: Vec<PathBuf>,
    pub crossing: Vec<FileChange>,
}

impl FolderChanges {
    #[must_use]
    pub fn len(&self) -> usize {
        self.tracked.len() + self.untracked.len() + self.crossing.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tracked.is_empty() && self.untracked.is_empty() && self.crossing.is_empty()
    }
}

#[must_use]
pub fn folder_changes(changes: &[FileChange], folder: &Path) -> FolderChanges {
    let inside = |path: &Path| path.starts_with(folder);
    let mut held = FolderChanges::default();
    for change in changes {
        let path_inside = inside(&change.path);
        let origin_inside = change.origin.as_deref().map(inside);
        if change.kind == ChangeKind::Renamed
            && origin_inside.is_some_and(|origin| origin != path_inside)
        {
            held.crossing.push(change.clone());
            continue;
        }
        let restored = path_inside.then_some(change.path.as_path()).or_else(|| {
            change
                .origin
                .as_deref()
                .filter(|_| origin_inside == Some(true))
        });
        let Some(path) = restored else {
            continue;
        };
        match change.kind {
            ChangeKind::Untracked => held.untracked.push(path.to_path_buf()),
            _ => held.tracked.push(path.to_path_buf()),
        }
    }
    held
}

fn checked_folder(folder: &Path) -> Result<&Path, RepoError> {
    let mut components = folder.components().peekable();
    let plain = components.peek().is_some()
        && components.all(|component| matches!(component, Component::Normal(_)));
    if plain {
        Ok(folder)
    } else {
        Err(RepoError::OutsideRepository(folder.display().to_string()))
    }
}

fn literal_pathspec(path: &Path) -> OsString {
    let mut pathspec = OsString::from(LITERAL_PATHSPEC);
    pathspec.push(path.as_os_str());
    pathspec
}

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
    let root = required_repo_root(host, repo)?;
    discard_at_root(host, &root, change)
}

fn discard_at_root(host: &HostLink, repo: &Path, change: &FileChange) -> Result<(), RepoError> {
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

/// # Errors
///
/// Returns [`RepoError::OutsideRepository`] when the folder is not a plain path inside the
/// repository, [`RepoError::Git`] when git cannot read or restore it and [`RepoError::Io`] when
/// an untracked file cannot be removed. A rename across the folder's edge is undone whole, so
/// the file goes back to its original path.
pub fn discard_folder(
    host: &HostLink,
    repo: &Path,
    folder: &Path,
) -> Result<FolderChanges, RepoError> {
    let folder = checked_folder(folder)?;
    let root = required_repo_root(host, repo)?;
    let held = folder_changes(&status(host, &root)?.changes, folder);
    for change in &held.crossing {
        discard_at_root(host, &root, change)?;
    }
    if !held.tracked.is_empty() {
        let pathspec = literal_pathspec(folder);
        git_on_paths(host, &root, &RESTORE_ARGS, &[Path::new(&pathspec)])?;
    }
    for path in &held.untracked {
        delete_entry(host, &root.join(path))?;
    }
    Ok(held)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(path: &str, kind: ChangeKind, origin: Option<&str>) -> FileChange {
        FileChange {
            path: PathBuf::from(path),
            kind,
            origin: origin.map(PathBuf::from),
        }
    }

    #[test]
    fn a_folder_holds_the_changes_under_it_and_nothing_beside_it() {
        let changes = [
            change("src/a.rs", ChangeKind::Modified, None),
            change("src/new.rs", ChangeKind::Untracked, None),
            change("src/deep/b.rs", ChangeKind::Deleted, None),
            change("src2/c.rs", ChangeKind::Modified, None),
            change("docs/moved.md", ChangeKind::Renamed, Some("src/moved.md")),
            change("src/in.rs", ChangeKind::Renamed, Some("out.rs")),
            change("README.md", ChangeKind::Untracked, None),
        ];
        let held = folder_changes(&changes, Path::new("src"));
        assert_eq!(
            held.tracked,
            ["src/a.rs", "src/deep/b.rs"].map(PathBuf::from)
        );
        assert_eq!(held.untracked, [PathBuf::from("src/new.rs")]);
        assert_eq!(held.crossing, [changes[4].clone(), changes[5].clone()]);
        assert_eq!(held.len(), 5);
        assert_eq!(
            folder_changes(&changes, Path::new("lib")),
            FolderChanges::default()
        );
    }

    #[test]
    fn only_plain_folders_inside_the_repository_are_discarded() {
        assert!(checked_folder(Path::new("src/ui")).is_ok());
        for folder in ["", "..", "src/../..", "/etc", "./src"] {
            assert!(
                matches!(
                    checked_folder(Path::new(folder)),
                    Err(RepoError::OutsideRepository(_))
                ),
                "{folder}"
            );
        }
    }
}
