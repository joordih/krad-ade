use krad_domain::{ChatImage, EnvironmentName, ImageFormat, TerminalId};

use crate::archive::{forget_chat_files_under, ArchiveStoreError};
use std::fmt::Write as _;
use std::fs::{self, OpenOptions};
use std::io::{self, ErrorKind, Write as _};
use std::path::{Path, PathBuf};

use crate::config_dir::config_dir;

const CHAT_IMAGES_DIR_NAME: &str = "chat-images";
const FIRST_IMAGE_NUMBER: u64 = 1;
const SAVE_ATTEMPTS: u64 = 64;

#[derive(Debug, thiserror::Error)]
pub enum ChatImageError {
    #[error("Could not save the pasted image: {0}")]
    Save(io::Error),
    #[error("Could not delete the chat's pasted images: {0}")]
    Delete(io::Error),
}

#[derive(Debug, PartialEq, Eq)]
pub enum ImageUnread {
    Missing,
    Outside,
    TooLarge { len: u64, limit: u64 },
}

pub(crate) fn environment_folder(env: &EnvironmentName) -> String {
    env.as_str()
        .bytes()
        .fold(String::new(), |mut folder, byte| {
            write!(folder, "{byte:02x}").unwrap_or_default();
            folder
        })
}

pub(crate) fn images_folder(root: &Path, env: &EnvironmentName, chat: TerminalId) -> PathBuf {
    root.join(CHAT_IMAGES_DIR_NAME)
        .join(environment_folder(env))
        .join(chat.as_u64().to_string())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatImages {
    root: PathBuf,
}

impl ChatImages {
    #[must_use]
    pub const fn new(root: PathBuf) -> Self {
        Self { root }
    }

    #[must_use]
    pub fn in_config_folder() -> Option<Self> {
        config_dir().map(Self::new)
    }

    /// # Errors
    ///
    /// Fails when the image cannot be written.
    pub fn save(
        &self,
        env: &EnvironmentName,
        chat: TerminalId,
        format: ImageFormat,
        bytes: &[u8],
    ) -> Result<ChatImage, ChatImageError> {
        save_chat_image(&images_folder(&self.root, env, chat), format, bytes)
    }

    /// # Errors
    ///
    /// Fails when a file the app kept for the archived chat cannot be removed.
    pub fn forget_archived(
        &self,
        env: &EnvironmentName,
        chat: TerminalId,
    ) -> Result<(), ArchiveStoreError> {
        forget_chat_files_under(&self.root, env, chat)
    }

    /// # Errors
    ///
    /// Fails when the chat's folder exists and cannot be removed.
    pub fn forget(&self, env: &EnvironmentName, chat: TerminalId) -> Result<(), ChatImageError> {
        delete_chat_images(&images_folder(&self.root, env, chat))
    }

    /// # Errors
    ///
    /// Fails when the image is missing, lies outside this runtime's image folder or is larger
    /// than `limit`.
    pub fn read(&self, image: &ChatImage, limit: u64) -> Result<Vec<u8>, ImageUnread> {
        let folder = fs::canonicalize(self.root.join(CHAT_IMAGES_DIR_NAME))
            .map_err(|_| ImageUnread::Missing)?;
        let path = fs::canonicalize(&image.path).map_err(|_| ImageUnread::Missing)?;
        if !path.starts_with(&folder) {
            return Err(ImageUnread::Outside);
        }
        let len = fs::metadata(&path).map_err(|_| ImageUnread::Missing)?.len();
        if len > limit {
            return Err(ImageUnread::TooLarge { len, limit });
        }
        fs::read(&path).map_err(|_| ImageUnread::Missing)
    }
}

fn next_image_number<'a>(existing: impl IntoIterator<Item = &'a Path>) -> u64 {
    existing
        .into_iter()
        .filter_map(|path| path.file_stem()?.to_str()?.parse::<u64>().ok())
        .max()
        .map_or(FIRST_IMAGE_NUMBER, |last| last.saturating_add(1))
}

fn image_file(dir: &Path, number: u64, format: ImageFormat) -> PathBuf {
    dir.join(number.to_string())
        .with_extension(format.extension())
}

fn save_chat_image(
    dir: &Path,
    format: ImageFormat,
    bytes: &[u8],
) -> Result<ChatImage, ChatImageError> {
    fs::create_dir_all(dir).map_err(ChatImageError::Save)?;
    let existing: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(ChatImageError::Save)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    let first = next_image_number(existing.iter().map(PathBuf::as_path));
    for number in first..first.saturating_add(SAVE_ATTEMPTS) {
        let path = image_file(dir, number, format);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                file.write_all(bytes).map_err(ChatImageError::Save)?;
                return Ok(ChatImage { path, format });
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
            Err(error) => return Err(ChatImageError::Save(error)),
        }
    }
    Err(ChatImageError::Save(io::Error::from(
        ErrorKind::AlreadyExists,
    )))
}

fn delete_chat_images(dir: &Path) -> Result<(), ChatImageError> {
    match fs::remove_dir_all(dir) {
        Err(error) if error.kind() != ErrorKind::NotFound => Err(ChatImageError::Delete(error)),
        Ok(()) | Err(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("krad-chat-images-{name}"));
        fs::remove_dir_all(&dir).unwrap_or_default();
        dir
    }

    #[test]
    fn each_chat_keeps_its_images_in_its_own_folder() {
        let main = EnvironmentName::new("main".to_owned());
        let odd = EnvironmentName::new("a/../b".to_owned());
        assert_eq!(
            images_folder(Path::new("/config"), &main, TerminalId(7)),
            Path::new("/config/chat-images/6d61696e/7")
        );
        assert_eq!(
            images_folder(Path::new("/config"), &odd, TerminalId(7)),
            Path::new("/config/chat-images/612f2e2e2f62/7")
        );
    }

    #[test]
    fn pasted_images_are_numbered_after_the_last_one() {
        assert_eq!(next_image_number([]), 1);
        assert_eq!(
            next_image_number([
                Path::new("/d/1.png"),
                Path::new("/d/12.jpg"),
                Path::new("/d/notes.txt"),
            ]),
            13
        );
    }

    #[test]
    fn a_saved_image_goes_with_its_chat_folder() {
        let dir = scratch("save");
        let saved = |format, bytes: &[u8]| {
            save_chat_image(&dir, format, bytes)
                .unwrap_or_else(|_| unreachable!("the scratch folder is writable"))
        };
        let first = saved(ImageFormat::Png, b"one");
        let second = saved(ImageFormat::Jpeg, b"two");
        assert_eq!(first.path, dir.join("1.png"));
        assert_eq!(second.path, dir.join("2.jpg"));
        assert_eq!(second.format, ImageFormat::Jpeg);
        assert_eq!(fs::read(&first.path).unwrap_or_default(), b"one");
        assert!(delete_chat_images(&dir).is_ok());
        assert!(!dir.exists());
        assert!(delete_chat_images(&dir).is_ok(), "an absent folder is fine");
    }

    #[test]
    fn only_a_kept_image_small_enough_is_read_back() {
        let root = scratch("read");
        let images = ChatImages::new(root.clone());
        let main = EnvironmentName::new("main".to_owned());
        let saved = images
            .save(&main, TerminalId(3), ImageFormat::Png, b"pixels")
            .unwrap_or_else(|_| unreachable!("the scratch folder is writable"));
        assert_eq!(images.read(&saved, 64), Ok(b"pixels".to_vec()));
        assert_eq!(
            images.read(&saved, 2),
            Err(ImageUnread::TooLarge { len: 6, limit: 2 })
        );
        let outside = root.join("secret.png");
        fs::write(&outside, b"no").unwrap_or_default();
        let escape = ChatImage {
            path: images_folder(&root, &main, TerminalId(3)).join("../../../secret.png"),
            format: ImageFormat::Png,
        };
        assert_eq!(images.read(&escape, 64), Err(ImageUnread::Outside));
        let gone = ChatImage {
            path: images_folder(&root, &main, TerminalId(3)).join("9.png"),
            format: ImageFormat::Png,
        };
        assert_eq!(images.read(&gone, 64), Err(ImageUnread::Missing));
        fs::remove_dir_all(&root).unwrap_or_default();
    }

    #[test]
    fn a_runtime_keeps_and_forgets_a_chat_s_images_under_its_own_root() {
        let root = scratch("root");
        let images = ChatImages::new(root.clone());
        let main = EnvironmentName::new("main".to_owned());
        let saved = images
            .save(&main, TerminalId(2), ImageFormat::Png, b"one")
            .unwrap_or_else(|_| unreachable!("the scratch folder is writable"));
        assert_eq!(
            saved.path,
            images_folder(&root, &main, TerminalId(2)).join("1.png")
        );
        assert!(images.forget(&main, TerminalId(2)).is_ok());
        assert!(!saved.path.exists());
        fs::remove_dir_all(&root).unwrap_or_default();
    }
}
