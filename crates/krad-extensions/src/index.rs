mod cache;
mod entry;
mod install;
mod search;

pub use cache::{cached_index, load_index, Freshness, IndexOrigin, LoadedIndex, MAX_INDEX_AGE};
pub use entry::{Compatibility, EntryError, GitAddress, IndexEntry};
pub use install::{add_from_index, update_from_index};

use krad_domain::{ExtensionEntry, ExtensionSource, GitCommit};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use entry::RawEntry;

pub const DEFAULT_INDEX_REPOSITORY: &str = "joordih/krad-extensions-index";
const REPOSITORY_SEPARATOR: char = '/';
const REPOSITORY_PUNCTUATION: [char; 3] = ['-', '_', '.'];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexRepository(String);

impl FromStr for IndexRepository {
    type Err = IndexError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let text = text.trim();
        let part = |part: &str| {
            !part.is_empty()
                && !part.starts_with(REPOSITORY_PUNCTUATION)
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || REPOSITORY_PUNCTUATION.contains(&c))
        };
        match text.split_once(REPOSITORY_SEPARATOR) {
            Some((owner, name)) if part(owner) && part(name) => Ok(Self(text.to_owned())),
            _ => Err(IndexError::Repository(text.to_owned())),
        }
    }
}

impl Default for IndexRepository {
    fn default() -> Self {
        Self(DEFAULT_INDEX_REPOSITORY.to_owned())
    }
}

impl fmt::Display for IndexRepository {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    #[error("the extension index is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0} is not an owner/name GitHub repository")]
    Repository(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryProblem {
    pub position: usize,
    pub error: EntryError,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexUpdate {
    pub extension: String,
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
                let entry = self.get(&extension.id)?;
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
