use crate::config_dir::config_dir;
use crate::layout::write_atomically;
use krad_domain::{read_digests, write_digests, DigestBook, DigestsFileError};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

const CHAT_MEMORY_DIR: &str = "chat-memory";
pub(crate) const DIGESTS_FILE: &str = "digests.json";
const UNREADABLE_SUFFIX: &str = "unreadable";

#[derive(Debug, thiserror::Error)]
pub enum DigestStoreError {
    #[error("no config directory")]
    NoConfigDir,
    #[error("{0}")]
    File(#[from] DigestsFileError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub(crate) fn read_book(path: &Path) -> Result<DigestBook, DigestStoreError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(DigestBook::default()),
        Err(error) => return Err(error.into()),
    };
    match read_digests(&text) {
        Ok(book) => Ok(book),
        Err(error) => {
            std::fs::rename(path, path.with_extension(UNREADABLE_SUFFIX))?;
            Err(error.into())
        }
    }
}

pub(crate) fn write_book(path: &Path, book: &DigestBook) -> Result<(), DigestStoreError> {
    let text = write_digests(book)?;
    write_atomically(path, text.as_bytes())?;
    Ok(())
}

#[derive(Clone, Debug)]
pub struct OrdinaryDigests {
    path: PathBuf,
}

impl OrdinaryDigests {
    /// # Errors
    /// Fails when there is no config folder.
    pub fn in_config() -> Result<Self, DigestStoreError> {
        let dir = config_dir().ok_or(DigestStoreError::NoConfigDir)?;
        Ok(Self::in_folder(&dir))
    }

    #[must_use]
    pub fn in_folder(config: &Path) -> Self {
        Self {
            path: config.join(CHAT_MEMORY_DIR).join(DIGESTS_FILE),
        }
    }

    /// # Errors
    /// Fails when the file exists but cannot be read; an unreadable file is moved aside.
    pub fn read(&self) -> Result<DigestBook, DigestStoreError> {
        read_book(&self.path)
    }

    /// # Errors
    /// Fails when the digests cannot be written.
    pub fn write(&self, book: &DigestBook) -> Result<(), DigestStoreError> {
        write_book(&self.path, book)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{DigestChat, DigestDraft, EnvironmentName, SessionId, TerminalId};
    use std::time::{Duration, SystemTime};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn scratch(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "krad-store-chat-digests-{name}-{}",
            std::process::id()
        ))
    }

    fn book() -> DigestBook {
        let mut book = DigestBook::default();
        book.record(DigestDraft {
            chat: DigestChat {
                environment: EnvironmentName::new("main".to_owned()),
                session: SessionId(1),
                terminal: TerminalId(2),
                title: "Login".to_owned(),
                folder: None,
                resume: None,
            },
            facts: vec!["Login uses OAuth".to_owned()],
            covered: 2,
            written: SystemTime::UNIX_EPOCH + Duration::from_secs(5),
        });
        book
    }

    #[test]
    fn ordinary_digests_round_trip_in_the_chat_memory_folder() -> TestResult {
        let dir = scratch("round-trip");
        let shelf = OrdinaryDigests::in_folder(&dir);
        assert_eq!(shelf.read()?, DigestBook::default());
        shelf.write(&book())?;
        assert!(dir.join(CHAT_MEMORY_DIR).join(DIGESTS_FILE).is_file());
        assert_eq!(shelf.read()?, book());
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn a_broken_file_is_moved_aside_instead_of_lost() -> TestResult {
        let dir = scratch("broken");
        let shelf = OrdinaryDigests::in_folder(&dir);
        let folder = dir.join(CHAT_MEMORY_DIR);
        std::fs::create_dir_all(&folder)?;
        std::fs::write(folder.join(DIGESTS_FILE), b"[")?;
        assert!(shelf.read().is_err());
        assert!(folder.join("digests.unreadable").is_file());
        assert_eq!(shelf.read()?, DigestBook::default());
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }
}
