use krad_domain::{ChatImage, EnvironmentName, ImageFormat, TerminalId};
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
    #[error("There is no config folder to keep the pasted image in")]
    NoConfigDir,
    #[error("Could not save the pasted image: {0}")]
    Save(io::Error),
    #[error("Could not delete the chat's pasted images: {0}")]
    Delete(io::Error),
}

fn environment_folder(env: &EnvironmentName) -> String {
    env.as_str()
        .bytes()
        .fold(String::new(), |mut folder, byte| {
            write!(folder, "{byte:02x}").unwrap_or_default();
            folder
        })
}

fn images_folder(root: &Path, env: &EnvironmentName, chat: TerminalId) -> PathBuf {
    root.join(CHAT_IMAGES_DIR_NAME)
        .join(environment_folder(env))
        .join(chat.as_u64().to_string())
}

#[must_use]
pub fn chat_images_dir(env: &EnvironmentName, chat: TerminalId) -> Option<PathBuf> {
    config_dir().map(|root| images_folder(&root, env, chat))
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

/// # Errors
///
/// Fails when the folder exists and cannot be removed.
pub fn delete_chat_images(dir: &Path) -> Result<(), ChatImageError> {
    match fs::remove_dir_all(dir) {
        Err(error) if error.kind() != ErrorKind::NotFound => Err(ChatImageError::Delete(error)),
        Ok(()) | Err(_) => Ok(()),
    }
}

/// # Errors
///
/// Fails when there is no config folder or the image cannot be written.
pub fn save_pasted_image(
    env: &EnvironmentName,
    chat: TerminalId,
    format: ImageFormat,
    bytes: &[u8],
) -> Result<ChatImage, ChatImageError> {
    let dir = chat_images_dir(env, chat).ok_or(ChatImageError::NoConfigDir)?;
    save_chat_image(&dir, format, bytes)
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
}
