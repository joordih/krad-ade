use crate::acp_transcript::transcript_file;
use crate::chat_images::images_folder;
use crate::config_dir::config_dir;
use krad_domain::{EnvironmentName, TerminalId};
use std::fs;
use std::io::{self, ErrorKind};
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum ArchiveStoreError {
    #[error("There is no config folder holding the archived chat's files")]
    NoConfigDir,
    #[error("Could not delete an archived chat's files: {0}")]
    Delete(io::Error),
}

fn remove_file(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        done => done,
    }
}

fn remove_dir(path: &Path) -> io::Result<()> {
    match fs::remove_dir_all(path) {
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        done => done,
    }
}

fn forget_chat_files_under(
    root: &Path,
    env: &EnvironmentName,
    chat: TerminalId,
) -> Result<(), ArchiveStoreError> {
    remove_dir(&images_folder(root, env, chat)).map_err(ArchiveStoreError::Delete)?;
    remove_file(&transcript_file(root, env, chat)).map_err(ArchiveStoreError::Delete)
}

/// # Errors
///
/// Fails when there is no config folder or a file the app kept for the chat cannot be removed.
pub fn forget_archived_chat_files(
    env: &EnvironmentName,
    chat: TerminalId,
) -> Result<(), ArchiveStoreError> {
    let root = config_dir().ok_or(ArchiveStoreError::NoConfigDir)?;
    forget_chat_files_under(&root, env, chat)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("krad-archive-{name}-{}", std::process::id()));
        fs::remove_dir_all(&dir).unwrap_or_default();
        dir
    }

    #[test]
    fn forgetting_a_chat_removes_its_images_and_transcript_only() -> io::Result<()> {
        let root = scratch("forget");
        let env = EnvironmentName::new("main".to_owned());
        let images = images_folder(&root, &env, TerminalId(4));
        let transcript = transcript_file(&root, &env, TerminalId(4));
        let neighbour = images_folder(&root, &env, TerminalId(5));
        for dir in [&images, &neighbour] {
            fs::create_dir_all(dir)?;
            fs::write(dir.join("1.png"), b"image")?;
        }
        if let Some(parent) = transcript.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&transcript, b"{}\n")?;
        forget_chat_files_under(&root, &env, TerminalId(4)).map_err(io::Error::other)?;
        assert!(!images.exists());
        assert!(!transcript.exists());
        assert!(neighbour.join("1.png").exists());
        forget_chat_files_under(&root, &env, TerminalId(4)).map_err(io::Error::other)?;
        fs::remove_dir_all(&root)
    }
}
