mod cache;
mod entry;
mod install;
mod search;

pub use cache::{cached_index, load_index, Freshness, IndexOrigin, LoadedIndex, MAX_INDEX_AGE};
pub use entry::{Compatibility, EntryError, GitAddress, IndexEntry};
pub use install::{add_from_index, update_from_index};

pub use krad_domain::{IndexRepository, DEFAULT_INDEX_REPOSITORY};

use krad_domain::{ExtensionEntry, ExtensionSource, GitCommit};
use serde::Deserialize;
use std::collections::BTreeSet;

use entry::RawEntry;

#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    #[error("the extension index is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryProblem {
    pub position: usize,
    pub error: EntryError,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexUpdate {
    pub extension: krad_domain::ExtensionId,
    pub installed: GitCommit,
    pub available: GitCommit,
    pub compatibility: Compatibility,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExtensionIndex {
    pub entries: Vec<IndexEntry>,
    pub problems: Vec<EntryProblem>,
}

#[derive(Deserialize)]
struct RawIndex {
    extensions: Vec<RawEntry>,
}

impl ExtensionIndex {
    /// # Errors
    ///
    /// Returns [`IndexError::Json`] when the text is not an index at all; entries that fail their
    /// checks are left out and listed in `problems`.
    pub fn parse(text: &str) -> Result<Self, IndexError> {
        let raw: RawIndex = serde_json::from_str(text)?;
        let mut index = Self::default();
        let mut seen = BTreeSet::new();
        for (position, raw) in raw.extensions.into_iter().enumerate() {
            let checked = IndexEntry::try_from(raw).and_then(|entry| {
                if seen.insert(entry.id.clone()) {
                    Ok(entry)
                } else {
                    Err(EntryError::Duplicate(entry.id))
                }
            });
            match checked {
                Ok(entry) => index.entries.push(entry),
                Err(error) => index.problems.push(EntryProblem { position, error }),
            }
        }
        Ok(index)
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&IndexEntry> {
        self.entries.iter().find(|entry| entry.id.as_ref() == id)
    }

    #[must_use]
    pub fn search(&self, query: &str) -> Vec<&IndexEntry> {
        search::search(&self.entries, query)
    }

    #[must_use]
    pub fn updates(&self, installed: &[ExtensionEntry]) -> Vec<IndexUpdate> {
        installed
            .iter()
            .filter_map(|extension| {
                let ExtensionSource::Pinned { commit, .. } = &extension.source else {
                    return None;
                };
                let entry = self.get(extension.id.as_ref())?;
                (entry.commit != *commit).then(|| IndexUpdate {
                    extension: extension.id.clone(),
                    installed: commit.clone(),
                    available: entry.commit.clone(),
                    compatibility: entry.compatibility(),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
