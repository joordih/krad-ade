use super::{KraddieShelf, KraddieStoreError};
use crate::chat_digests::{read_book, write_book, DigestStoreError, DIGESTS_FILE};
use crate::layout::write_atomically;
use krad_domain::{
    DigestBook, KraddieActivity, KraddieCompass, KraddieId, KraddieMemory, KraddieMind,
    COMPASS_FILE,
};
use std::io::ErrorKind;
use std::path::PathBuf;

const MEMORY_FILE: &str = "memory.json";
const ACTIVITY_FILE: &str = "activity.json";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KraddieProfileFiles {
    pub mind: KraddieMind,
    pub activity: KraddieActivity,
}

impl KraddieShelf {
    fn profile_dir(&self, id: &KraddieId) -> PathBuf {
        self.dir.join(id.as_str())
    }

    #[must_use]
    pub fn read_profile(&self, id: &KraddieId) -> KraddieProfileFiles {
        let dir = self.profile_dir(id);
        let compass = std::fs::read_to_string(dir.join(COMPASS_FILE))
            .ok()
            .and_then(|text| KraddieCompass::try_from(text).ok())
            .unwrap_or_default();
        let memory = std::fs::read(dir.join(MEMORY_FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<KraddieMemory>(&bytes).ok())
            .unwrap_or_default()
            .sanitized();
        let activity: KraddieActivity = std::fs::read(dir.join(ACTIVITY_FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        KraddieProfileFiles {
            mind: KraddieMind { compass, memory },
            activity,
        }
    }

    /// # Errors
    /// Fails when the compass cannot be written.
    pub fn write_compass(
        &self,
        id: &KraddieId,
        compass: &KraddieCompass,
    ) -> Result<(), KraddieStoreError> {
        let path = self.profile_dir(id).join(COMPASS_FILE);
        write_atomically(&path, compass.as_str().as_bytes())?;
        Ok(())
    }

    /// # Errors
    /// Fails when the memory cannot be written.
    pub fn write_memory(
        &self,
        id: &KraddieId,
        memory: &KraddieMemory,
    ) -> Result<(), KraddieStoreError> {
        let bytes = serde_json::to_vec_pretty(memory).map_err(std::io::Error::other)?;
        write_atomically(&self.profile_dir(id).join(MEMORY_FILE), &bytes)?;
        Ok(())
    }

    /// # Errors
    /// Fails when the file exists but cannot be read; an unreadable file is moved aside.
    pub fn read_digests(&self, id: &KraddieId) -> Result<DigestBook, DigestStoreError> {
        read_book(&self.profile_dir(id).join(DIGESTS_FILE))
    }

    /// # Errors
    /// Fails when the digests cannot be written.
    pub fn write_digests(&self, id: &KraddieId, book: &DigestBook) -> Result<(), DigestStoreError> {
        write_book(&self.profile_dir(id).join(DIGESTS_FILE), book)
    }

    /// # Errors
    /// Fails when the activity cannot be written.
    pub fn write_activity(
        &self,
        id: &KraddieId,
        activity: &KraddieActivity,
    ) -> Result<(), KraddieStoreError> {
        let bytes = serde_json::to_vec(activity).map_err(std::io::Error::other)?;
        write_atomically(&self.profile_dir(id).join(ACTIVITY_FILE), &bytes)?;
        Ok(())
    }

    /// # Errors
    /// Fails when the old profile folder exists but cannot be moved.
    pub fn rename_profile(
        &self,
        old: &KraddieId,
        new: &KraddieId,
    ) -> Result<(), KraddieStoreError> {
        match std::fs::rename(self.profile_dir(old), self.profile_dir(new)) {
            Err(error) if error.kind() != ErrorKind::NotFound => Err(error.into()),
            _ => Ok(()),
        }
    }

    pub(super) fn delete_profile(&self, id: &KraddieId) -> Result<(), KraddieStoreError> {
        match std::fs::remove_dir_all(self.profile_dir(id)) {
            Err(error) if error.kind() != ErrorKind::NotFound => Err(error.into()),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{KraddieDate, KraddieDay};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn scratch(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "krad-store-kraddie-profile-{name}-{}",
            std::process::id()
        ))
    }

    #[test]
    fn compass_memory_and_activity_live_in_the_kraddies_folder() -> TestResult {
        let dir = scratch("files");
        let shelf = KraddieShelf::at(dir.clone());
        let id = KraddieId::try_from("night-owl".to_owned())?;
        let today = "2026-11-09".parse::<KraddieDate>()?.0;
        assert_eq!(shelf.read_profile(&id), KraddieProfileFiles::default());
        let compass = KraddieCompass::try_from("# Compass\n\nBe brief.".to_owned())?;
        shelf.write_compass(&id, &compass)?;
        let mut memory = KraddieMemory::default();
        memory.add("Deploys happen on Fridays")?;
        shelf.write_memory(&id, &memory)?;
        let mut activity = KraddieActivity::default();
        let old = "2026-01-02".parse::<KraddieDate>()?.0;
        let one = KraddieDay {
            runs: 1,
            ..KraddieDay::default()
        };
        activity.record(old, &one);
        activity.record(today, &one);
        shelf.write_activity(&id, &activity)?;
        assert_eq!(
            std::fs::read_to_string(dir.join("night-owl").join("compass.md"))?,
            "# Compass\n\nBe brief."
        );
        let mut digests = DigestBook::default();
        digests.record(krad_domain::DigestDraft {
            chat: krad_domain::DigestChat {
                environment: krad_domain::EnvironmentName::new("main".to_owned()),
                session: krad_domain::SessionId(1),
                terminal: krad_domain::TerminalId(3),
                title: "Deploys".to_owned(),
                folder: None,
                resume: None,
            },
            facts: vec!["Deploys need a green CI".to_owned()],
            covered: 2,
            written: std::time::SystemTime::UNIX_EPOCH,
        });
        assert_eq!(shelf.read_digests(&id)?, DigestBook::default());
        shelf.write_digests(&id, &digests)?;
        assert!(dir.join("night-owl").join("digests.json").is_file());
        let read = shelf.read_profile(&id);
        assert_eq!(read.mind.compass, compass);
        assert_eq!(read.mind.memory, memory);
        assert_eq!(read.activity.len(), 1);
        let renamed = KraddieId::try_from("owl".to_owned())?;
        shelf.rename_profile(&id, &renamed)?;
        shelf.rename_profile(&id, &renamed)?;
        assert_eq!(shelf.read_profile(&renamed).mind.memory, memory);
        assert_eq!(shelf.read_digests(&renamed)?, digests);
        shelf.delete(&renamed)?;
        assert_eq!(shelf.read_digests(&renamed)?, DigestBook::default());
        assert_eq!(shelf.read_profile(&renamed), KraddieProfileFiles::default());
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn broken_profile_files_read_as_empty() -> TestResult {
        let dir = scratch("broken");
        let shelf = KraddieShelf::at(dir.clone());
        let id = KraddieId::try_from("junk".to_owned())?;
        std::fs::create_dir_all(dir.join("junk"))?;
        std::fs::write(dir.join("junk").join(MEMORY_FILE), b"[")?;
        std::fs::write(dir.join("junk").join(ACTIVITY_FILE), b"{\"days\":7}")?;
        let read = shelf.read_profile(&id);
        assert_eq!(read, KraddieProfileFiles::default());
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }
}
