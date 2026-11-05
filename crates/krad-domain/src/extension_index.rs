use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;
use std::str::FromStr;

pub const DEFAULT_INDEX_REPOSITORY: &str = "joordih/krad-extensions-index";
const REPOSITORY_SEPARATOR: char = '/';
const REPOSITORY_PUNCTUATION: [char; 3] = ['-', '_', '.'];

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct IndexRepository(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexRepositoryError(pub String);

impl fmt::Display for IndexRepositoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} is not an owner/name GitHub repository", self.0)
    }
}

impl std::error::Error for IndexRepositoryError {}

fn is_part(part: &str) -> bool {
    !part.is_empty()
        && !part.starts_with(REPOSITORY_PUNCTUATION)
        && part
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || REPOSITORY_PUNCTUATION.contains(&c))
}

impl FromStr for IndexRepository {
    type Err = IndexRepositoryError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let text = text.trim();
        match text.split_once(REPOSITORY_SEPARATOR) {
            Some((owner, name)) if is_part(owner) && is_part(name) => Ok(Self(text.to_owned())),
            _ => Err(IndexRepositoryError(text.to_owned())),
        }
    }
}

impl TryFrom<String> for IndexRepository {
    type Error = IndexRepositoryError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<IndexRepository> for String {
    fn from(repository: IndexRepository) -> Self {
        repository.0
    }
}

impl Default for IndexRepository {
    fn default() -> Self {
        Self(DEFAULT_INDEX_REPOSITORY.to_owned())
    }
}

impl IndexRepository {
    #[must_use]
    pub fn is_default(&self) -> bool {
        self.0 == DEFAULT_INDEX_REPOSITORY
    }
}

impl fmt::Display for IndexRepository {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

pub(crate) fn lenient_index_repository<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<IndexRepository, D::Error> {
    let text = String::deserialize(deserializer)?;
    Ok(text.parse().unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Settings;

    #[test]
    fn index_repositories_are_owner_slash_name() {
        assert!("joordih/krad-extensions-index"
            .parse::<IndexRepository>()
            .is_ok());
        assert_eq!(
            IndexRepository::default().to_string(),
            DEFAULT_INDEX_REPOSITORY
        );
        for bad in ["joordih", "a/b/c", "../x", "a/-x", "a /b"] {
            assert!(bad.parse::<IndexRepository>().is_err(), "{bad}");
        }
    }

    #[test]
    fn settings_keep_a_chosen_index_and_shrug_off_a_bad_one() -> serde_json::Result<()> {
        let chosen: Settings = serde_json::from_str(r#"{"extension_index":"acme/extensions"}"#)?;
        assert_eq!(chosen.extension_index.to_string(), "acme/extensions");
        let bad: Settings = serde_json::from_str(r#"{"extension_index":"not a repo"}"#)?;
        assert!(bad.extension_index.is_default());
        let text = serde_json::to_string(&Settings::default())?;
        assert!(!text.contains("extension_index"));
        Ok(())
    }
}
