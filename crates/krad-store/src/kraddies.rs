mod folder;
mod migration;
mod profile;
mod rename;

pub use profile::KraddieProfileFiles;
pub use rename::KraddieRename;

use crate::config_dir::config_dir;
use crate::layout::write_atomically;
use krad_domain::{
    Automation, AutomationKey, AutomationsFile, AutomationsFileError, Kraddie, KraddieFile,
    KraddieFileError, KraddieHistory, KraddieId, ReadKraddie, KRADDIE_FILE_EXTENSION,
    LEGACY_KRADDIE_FILE_EXTENSION,
};
use std::path::{Path, PathBuf};

const NAME_SEPARATOR: char = '.';
const HISTORY_SUFFIX: &str = "runs";
const AUTOMATIONS_SUFFIX: &str = "automations";
const JSON_EXTENSION: &str = "json";

#[derive(Debug, thiserror::Error)]
pub enum KraddieStoreError {
    #[error("no config directory")]
    NoConfigDir,
    #[error("{0}")]
    File(#[from] KraddieFileError),
    #[error("{0}")]
    Automations(#[from] AutomationsFileError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShelvedAutomation {
    pub automation: Automation,
    pub history: KraddieHistory,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShelvedKraddie {
    pub kraddie: Kraddie,
    pub automations: Vec<ShelvedAutomation>,
    pub migration_problem: Option<String>,
}

#[derive(Clone, Debug)]
pub struct KraddieShelf {
    dir: PathBuf,
}

impl KraddieShelf {
    #[must_use]
    pub fn in_config() -> Option<Self> {
        config_dir().map(|dir| Self::at(dir.join(folder::KRADDIES_DIR)))
    }

    #[must_use]
    pub const fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn kraddie_path(&self, id: &KraddieId) -> PathBuf {
        self.dir
            .join(id.as_str())
            .with_extension(KRADDIE_FILE_EXTENSION)
    }

    fn sibling(&self, id: &KraddieId, part: &str) -> PathBuf {
        self.dir.join(format!(
            "{id}{NAME_SEPARATOR}{part}{NAME_SEPARATOR}{JSON_EXTENSION}"
        ))
    }

    fn automations_path(&self, id: &KraddieId) -> PathBuf {
        self.sibling(id, AUTOMATIONS_SUFFIX)
    }

    fn history_path(&self, key: &AutomationKey) -> PathBuf {
        self.sibling(
            &key.kraddie,
            &format!("{}{NAME_SEPARATOR}{HISTORY_SUFFIX}", key.automation),
        )
    }

    fn legacy_history_path(&self, id: &KraddieId) -> PathBuf {
        self.dir.join(id.as_str()).with_extension(JSON_EXTENSION)
    }

    #[must_use]
    pub fn settle_folder(&self) -> Vec<std::io::Error> {
        folder::settle(&self.dir)
    }

    #[must_use]
    pub fn read_all(&self) -> Vec<ShelvedKraddie> {
        let leftover = folder::leftover(&self.dir).map(Self::at);
        let mut kraddies = self.read_folder();
        for shelved in leftover.iter().flat_map(Self::read_folder) {
            let id = shelved.kraddie.id();
            if !kraddies.iter().any(|known| known.kraddie.id() == id) {
                kraddies.push(shelved);
            }
        }
        kraddies.sort_by_key(|shelved| shelved.kraddie.name.as_str().to_lowercase());
        kraddies
    }

    fn read_folder(&self) -> Vec<ShelvedKraddie> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| is_kraddie_file(path))
            .collect();
        paths.sort_by_key(|path| folder::is_legacy_kraddie_file(path));
        let mut kraddies: Vec<ShelvedKraddie> = Vec::new();
        for read in paths.iter().filter_map(|path| read_kraddie(path).ok()) {
            let id = read.kraddie.id();
            if kraddies.iter().all(|known| known.kraddie.id() != id) {
                kraddies.push(self.shelved(read));
            }
        }
        kraddies
    }

    fn shelved(&self, read: ReadKraddie) -> ShelvedKraddie {
        let id = read.kraddie.id();
        let mut migration_problem = None;
        let automations = match self.read_automations(&id) {
            Some(Ok(automations)) => automations,
            Some(Err(error)) => {
                migration_problem = Some(error.to_string());
                read.automations
            }
            None => {
                migration_problem = self.adopt(&read).err().map(|error| error.to_string());
                read.automations
            }
        };
        let automations = automations
            .into_iter()
            .map(|automation| {
                let key = AutomationKey::new(id.clone(), automation.id);
                ShelvedAutomation {
                    history: self.read_history(&key),
                    automation,
                }
            })
            .collect();
        ShelvedKraddie {
            kraddie: read.kraddie,
            automations,
            migration_problem,
        }
    }

    fn read_automations(
        &self,
        id: &KraddieId,
    ) -> Option<Result<Vec<Automation>, KraddieStoreError>> {
        let text = match std::fs::read_to_string(self.automations_path(id)) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
            Err(error) => return Some(Err(error.into())),
        };
        Some(AutomationsFile::read(&text).map_err(KraddieStoreError::from))
    }

    fn read_history(&self, key: &AutomationKey) -> KraddieHistory {
        std::fs::read(self.history_path(key))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// # Errors
    /// Fails when the kraddie cannot be written.
    pub fn write(&self, kraddie: &Kraddie) -> Result<(), KraddieStoreError> {
        let text = KraddieFile::new(kraddie.clone(), Vec::new()).write()?;
        let path = self.kraddie_path(&kraddie.id());
        write_atomically(&path, text.as_bytes())?;
        remove_if_present(&path.with_extension(LEGACY_KRADDIE_FILE_EXTENSION))
    }

    /// # Errors
    /// Fails when the automations cannot be written.
    pub fn write_automations(
        &self,
        kraddie: &KraddieId,
        automations: &[Automation],
    ) -> Result<(), KraddieStoreError> {
        let text = AutomationsFile::new(automations.to_vec()).write()?;
        write_atomically(&self.automations_path(kraddie), text.as_bytes())?;
        Ok(())
    }

    /// # Errors
    /// Fails when the history cannot be written.
    pub fn write_history(
        &self,
        key: &AutomationKey,
        history: &KraddieHistory,
    ) -> Result<(), KraddieStoreError> {
        let bytes = serde_json::to_vec(history).map_err(std::io::Error::other)?;
        write_atomically(&self.history_path(key), &bytes)?;
        Ok(())
    }

    /// # Errors
    /// Fails when the automation's history exists but cannot be removed.
    pub fn delete_history(&self, key: &AutomationKey) -> Result<(), KraddieStoreError> {
        remove_if_present(&self.history_path(key))
    }

    /// # Errors
    /// Fails when the kraddie's files exist but cannot be removed.
    pub fn delete(&self, id: &KraddieId) -> Result<(), KraddieStoreError> {
        self.delete_files(id)?;
        self.delete_profile(id)
    }

    fn delete_files(&self, id: &KraddieId) -> Result<(), KraddieStoreError> {
        let histories = self.history_files(id);
        let path = self.kraddie_path(id);
        let own = [
            path.with_extension(LEGACY_KRADDIE_FILE_EXTENSION),
            path,
            self.automations_path(id),
            self.legacy_history_path(id),
        ];
        for path in own.into_iter().chain(histories) {
            remove_if_present(&path)?;
        }
        Ok(())
    }

    fn history_files(&self, id: &KraddieId) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| history_owner(path).as_ref() == Some(id))
            .collect()
    }
}

fn history_owner(path: &Path) -> Option<KraddieId> {
    let name = path.file_name()?.to_str()?;
    let stem = name
        .strip_suffix(JSON_EXTENSION)?
        .strip_suffix(NAME_SEPARATOR)?;
    let stem = stem
        .strip_suffix(HISTORY_SUFFIX)?
        .strip_suffix(NAME_SEPARATOR)?;
    let (kraddie, automation) = stem.rsplit_once(NAME_SEPARATOR)?;
    automation.parse::<u32>().ok()?;
    KraddieId::try_from(kraddie.to_owned()).ok()
}

fn remove_if_present(path: &Path) -> Result<(), KraddieStoreError> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
        _ => Ok(()),
    }
}

fn is_kraddie_file(path: &Path) -> bool {
    folder::is_legacy_kraddie_file(path)
        || path
            .extension()
            .is_some_and(|extension| extension == KRADDIE_FILE_EXTENSION)
}

fn read_kraddie(path: &Path) -> Result<ReadKraddie, KraddieStoreError> {
    let text = std::fs::read_to_string(path)?;
    Ok(KraddieFile::read(&text)?)
}

/// # Errors
/// Fails when the file cannot be written.
pub fn export_kraddie(
    kraddie: &Kraddie,
    automations: &[Automation],
    path: &Path,
) -> Result<(), KraddieStoreError> {
    let automations = automations.iter().map(Automation::exported).collect();
    let text = KraddieFile::new(kraddie.exported(), automations).write()?;
    write_atomically(path, text.as_bytes())?;
    Ok(())
}

/// # Errors
/// Fails when the file cannot be read or is not a valid kraddie file.
pub fn import_kraddie(path: &Path) -> Result<ReadKraddie, KraddieStoreError> {
    read_kraddie(path)
}

#[cfg(test)]
mod tests;
