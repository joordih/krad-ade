use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};

use crate::RepoError;

const CURRENT_DIR: &str = ".";
const PARENT_DIR: &str = "..";

/// # Errors
/// Returns [`RepoError::InvalidName`] when `new_name` is empty or holds a path separator,
/// [`RepoError::AlreadyExists`] when a sibling already has that name, and
/// [`RepoError::Io`] when the rename fails.
pub fn rename_entry(path: &Path, new_name: &str) -> Result<PathBuf, RepoError> {
    let parent = path
        .parent()
        .ok_or_else(|| RepoError::InvalidName(path.display().to_string()))?;
    let target = child_path(parent, new_name)?;
    if target == path {
        return Ok(target);
    }
    move_to(path, target)
}

/// # Errors
/// Returns [`RepoError::MoveIntoItself`] when `dest_dir` is `path` or lies inside it,
/// [`RepoError::AlreadyExists`] when `dest_dir` already holds an entry with that name, and
/// [`RepoError::Io`] when the move fails.
pub fn move_entry(path: &Path, dest_dir: &Path) -> Result<PathBuf, RepoError> {
    if dest_dir.starts_with(path) {
        return Err(RepoError::MoveIntoItself);
    }
    let name = path
        .file_name()
        .ok_or_else(|| RepoError::InvalidName(path.display().to_string()))?;
    let target = dest_dir.join(name);
    if target == path {
        return Ok(target);
    }
    move_to(path, target)
}

/// # Errors
/// Returns [`RepoError::Io`] when the entry cannot be read or removed.
pub fn delete_entry(path: &Path) -> Result<(), RepoError> {
    if fs::symlink_metadata(path)?.is_dir() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// # Errors
/// Returns [`RepoError::InvalidName`] for an empty name or one with a path separator,
/// [`RepoError::AlreadyExists`] when the name is taken, and [`RepoError::Io`] otherwise.
pub fn create_file(dir: &Path, name: &str) -> Result<PathBuf, RepoError> {
    let path = vacant_child(dir, name)?;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    Ok(path)
}

/// # Errors
/// Returns [`RepoError::InvalidName`] for an empty name or one with a path separator,
/// [`RepoError::AlreadyExists`] when the name is taken, and [`RepoError::Io`] otherwise.
pub fn create_dir(dir: &Path, name: &str) -> Result<PathBuf, RepoError> {
    let path = vacant_child(dir, name)?;
    fs::create_dir(&path)?;
    Ok(path)
}

fn move_to(path: &Path, target: PathBuf) -> Result<PathBuf, RepoError> {
    if is_taken(&target, path)? {
        return Err(already_exists(&target));
    }
    fs::rename(path, &target)?;
    Ok(target)
}

fn vacant_child(dir: &Path, name: &str) -> Result<PathBuf, RepoError> {
    let path = child_path(dir, name)?;
    if fs::symlink_metadata(&path).is_ok() {
        return Err(already_exists(&path));
    }
    Ok(path)
}

fn is_taken(target: &Path, source: &Path) -> Result<bool, RepoError> {
    if fs::symlink_metadata(target).is_err() {
        return Ok(false);
    }
    Ok(!same_entry(target, source)?)
}

#[cfg(unix)]
fn same_entry(left: &Path, right: &Path) -> Result<bool, RepoError> {
    use std::os::unix::fs::MetadataExt;
    let (left, right) = (fs::symlink_metadata(left)?, fs::symlink_metadata(right)?);
    Ok(left.dev() == right.dev() && left.ino() == right.ino())
}

#[cfg(not(unix))]
fn same_entry(left: &Path, right: &Path) -> Result<bool, RepoError> {
    Ok(fs::canonicalize(left)? == fs::canonicalize(right)?)
}

fn child_path(dir: &Path, name: &str) -> Result<PathBuf, RepoError> {
    let name = name.trim();
    let mut components = Path::new(name).components();
    let single = components.next().is_some() && components.next().is_none();
    if !single || name == CURRENT_DIR || name == PARENT_DIR || name.contains(['/', '\\']) {
        return Err(RepoError::InvalidName(name.to_owned()));
    }
    Ok(dir.join(name))
}

fn already_exists(path: &Path) -> RepoError {
    RepoError::AlreadyExists(
        path.file_name()
            .map_or_default(|name| name.to_string_lossy().into_owned()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(label: &str) -> Result<PathBuf, Box<dyn Error>> {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "krad-repo-ops-{label}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    #[test]
    fn rename_keeps_the_entry_in_its_folder() -> Result<(), Box<dyn Error>> {
        let dir = unique_temp_dir("rename")?;
        fs::write(dir.join("old.txt"), "body")?;
        fs::write(dir.join("taken.txt"), "")?;

        let renamed = rename_entry(&dir.join("old.txt"), " new.txt ");
        let clash = rename_entry(&dir.join("new.txt"), "taken.txt");
        let escape = rename_entry(&dir.join("new.txt"), "../out.txt");
        let body = fs::read_to_string(dir.join("new.txt"));
        fs::remove_dir_all(&dir)?;

        assert_eq!(renamed?, dir.join("new.txt"));
        assert_eq!(body?, "body");
        assert!(matches!(clash, Err(RepoError::AlreadyExists(name)) if name == "taken.txt"));
        assert!(matches!(escape, Err(RepoError::InvalidName(_))));
        Ok(())
    }

    #[test]
    fn create_rejects_taken_and_invalid_names() -> Result<(), Box<dyn Error>> {
        let dir = unique_temp_dir("create")?;

        let file = create_file(&dir, "notes.md");
        let folder = create_dir(&dir, "src");
        let taken = create_file(&dir, "src");
        let empty = create_dir(&dir, "  ");
        let parent = create_dir(&dir, "..");
        let is_file = dir.join("notes.md").is_file();
        let is_dir = dir.join("src").is_dir();
        fs::remove_dir_all(&dir)?;

        assert_eq!(file?, dir.join("notes.md"));
        assert_eq!(folder?, dir.join("src"));
        assert!(is_file && is_dir);
        assert!(matches!(taken, Err(RepoError::AlreadyExists(_))));
        assert!(matches!(empty, Err(RepoError::InvalidName(_))));
        assert!(matches!(parent, Err(RepoError::InvalidName(_))));
        Ok(())
    }

    #[test]
    fn move_carries_a_folder_but_never_into_itself() -> Result<(), Box<dyn Error>> {
        let dir = unique_temp_dir("move")?;
        fs::create_dir_all(dir.join("a/inner"))?;
        fs::write(dir.join("a/inner/file.rs"), "")?;
        fs::create_dir(dir.join("b"))?;

        let into_self = move_entry(&dir.join("a"), &dir.join("a/inner"));
        let moved = move_entry(&dir.join("a"), &dir.join("b"));
        let carried = dir.join("b/a/inner/file.rs").is_file();
        fs::remove_dir_all(&dir)?;

        assert!(matches!(into_self, Err(RepoError::MoveIntoItself)));
        assert_eq!(moved?, dir.join("b/a"));
        assert!(carried);
        Ok(())
    }

    #[test]
    fn delete_removes_files_and_whole_folders() -> Result<(), Box<dyn Error>> {
        let dir = unique_temp_dir("delete")?;
        fs::create_dir_all(dir.join("folder/nested"))?;
        fs::write(dir.join("folder/nested/file.txt"), "")?;
        fs::write(dir.join("loose.txt"), "")?;

        let folder = delete_entry(&dir.join("folder"));
        let file = delete_entry(&dir.join("loose.txt"));
        let left = fs::read_dir(&dir)?.count();
        fs::remove_dir_all(&dir)?;

        folder?;
        file?;
        assert_eq!(left, 0);
        Ok(())
    }
}
