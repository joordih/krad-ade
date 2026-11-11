use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use krad_domain::BackdropPlace;

use crate::config_dir::config_dir;

const WINDOW_DIR_NAME: &str = "backdrop";
const CHAT_DIR_NAME: &str = "chat-backdrop";
const BACKDROP_STEM: &str = "image";
const NAME_SEPARATOR: char = '-';
const FIRST_NUMBER: u64 = 1;

#[derive(Debug, thiserror::Error)]
pub enum BackdropImageError {
    #[error("There is no config folder to keep the backdrop image in")]
    NoConfigDir,
    #[error("Could not copy the backdrop image: {0}")]
    Copy(io::Error),
    #[error("Could not remove the backdrop image: {0}")]
    Remove(io::Error),
}

const fn dir_name(place: BackdropPlace) -> &'static str {
    match place {
        BackdropPlace::Window => WINDOW_DIR_NAME,
        BackdropPlace::Chat => CHAT_DIR_NAME,
    }
}

#[must_use]
pub fn backdrop_image_dir(place: BackdropPlace) -> Option<PathBuf> {
    config_dir().map(|root| root.join(dir_name(place)))
}

fn copy_name(source: &Path, number: u64) -> PathBuf {
    let name = PathBuf::from(format!("{BACKDROP_STEM}{NAME_SEPARATOR}{number}"));
    match source.extension() {
        Some(extension) => name.with_extension(extension),
        None => name,
    }
}

fn copy_number(path: &Path) -> Option<u64> {
    path.file_stem()?
        .to_str()?
        .strip_prefix(BACKDROP_STEM)?
        .strip_prefix(NAME_SEPARATOR)?
        .parse()
        .ok()
}

fn copy_into(dir: &Path, source: &Path) -> Result<PathBuf, BackdropImageError> {
    fs::create_dir_all(dir).map_err(BackdropImageError::Copy)?;
    let older: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(BackdropImageError::Copy)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    let number = older
        .iter()
        .filter_map(|path| copy_number(path))
        .max()
        .map_or(FIRST_NUMBER, |last| last.saturating_add(1));
    let copy = dir.join(copy_name(source, number));
    fs::copy(source, &copy).map_err(BackdropImageError::Copy)?;
    for path in older {
        remove_file(&path)?;
    }
    Ok(copy)
}

fn remove_file(path: &Path) -> Result<(), BackdropImageError> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != ErrorKind::NotFound => Err(BackdropImageError::Remove(error)),
        Ok(()) | Err(_) => Ok(()),
    }
}

/// # Errors
///
/// Fails when there is no config folder or the image cannot be copied into it.
pub fn import_backdrop_image(
    place: BackdropPlace,
    source: &Path,
) -> Result<PathBuf, BackdropImageError> {
    let dir = backdrop_image_dir(place).ok_or(BackdropImageError::NoConfigDir)?;
    copy_into(&dir, source)
}

/// # Errors
///
/// Fails when the copied image exists and cannot be removed.
pub fn forget_backdrop_image(place: BackdropPlace, copy: &Path) -> Result<(), BackdropImageError> {
    let Some(dir) = backdrop_image_dir(place) else {
        return Ok(());
    };
    if !copy.starts_with(&dir) {
        return Ok(());
    }
    remove_file(copy)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("krad-backdrop-{name}"));
        fs::remove_dir_all(&dir).unwrap_or_default();
        dir
    }

    #[test]
    fn the_copy_keeps_the_extension_and_replaces_the_previous_copy() {
        let dir = scratch("copy");
        let source_dir = scratch("source");
        fs::create_dir_all(&source_dir).unwrap_or_default();
        let source = source_dir.join("Sky Photo.JPG");
        fs::write(&source, b"pixels").unwrap_or_default();
        let first = copy_into(&dir, &source)
            .unwrap_or_else(|_| unreachable!("the scratch folder is writable"));
        assert_eq!(first, dir.join("image-1.JPG"));
        assert_eq!(copy_number(&first), Some(1));
        let second = copy_into(&dir, &source)
            .unwrap_or_else(|_| unreachable!("the scratch folder is writable"));
        assert_eq!(second, dir.join("image-2.JPG"));
        assert!(!first.exists());
        assert_eq!(fs::read(&second).unwrap_or_default(), b"pixels");
        fs::remove_file(&source).unwrap_or_default();
        assert!(second.exists(), "the copy outlives the original");
    }

    #[test]
    fn the_window_and_the_chat_copy_into_their_own_folders() {
        assert_ne!(
            dir_name(BackdropPlace::Window),
            dir_name(BackdropPlace::Chat)
        );
    }

    #[test]
    fn a_missing_source_is_an_error() {
        let dir = scratch("missing");
        assert!(matches!(
            copy_into(&dir, Path::new("/nowhere/at/all.png")),
            Err(BackdropImageError::Copy(_))
        ));
    }
}
