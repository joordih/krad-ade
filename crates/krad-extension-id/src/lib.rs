use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

pub const MAX_ID_LEN: usize = 64;
const ID_PUNCTUATION: [char; 3] = ['-', '_', '.'];
const DOT: char = '.';
const PARENT: &str = "..";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdError {
    Empty,
    TooLong,
    Character,
    Dots,
}

impl fmt::Display for IdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("an id cannot be empty"),
            Self::TooLong => write!(f, "an id is at most {MAX_ID_LEN} characters"),
            Self::Character => {
                f.write_str("an id holds only lowercase letters, digits, '-', '_' and '.'")
            }
            Self::Dots => f.write_str("an id cannot start with '.' or hold '..'"),
        }
    }
}

impl std::error::Error for IdError {}

/// # Errors
///
/// Returns [`IdError`] when the text is empty, too long, holds other characters than lowercase
/// letters, digits, '-', '_' and '.', starts with '.' or holds "..", so it is always one safe
/// path segment.
pub fn check_id(text: &str) -> Result<(), IdError> {
    if text.is_empty() {
        return Err(IdError::Empty);
    }
    if text.len() > MAX_ID_LEN {
        return Err(IdError::TooLong);
    }
    let allowed =
        |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || ID_PUNCTUATION.contains(&c);
    if !text.chars().all(allowed) {
        return Err(IdError::Character);
    }
    if text.starts_with(DOT) || text.contains(PARENT) {
        return Err(IdError::Dots);
    }
    Ok(())
}

/// # Errors
///
/// Returns [`IdError::Empty`] when the text is empty.
pub fn check_key(text: &str) -> Result<(), IdError> {
    if text.is_empty() {
        Err(IdError::Empty)
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ExtensionId(String);

impl ExtensionId {
    #[must_use]
    pub fn folder_in(&self, root: &Path) -> PathBuf {
        root.join(&self.0)
    }
}

impl TryFrom<String> for ExtensionId {
    type Error = IdError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        check_id(&text)?;
        Ok(Self(text))
    }
}

impl FromStr for ExtensionId {
    type Err = IdError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::try_from(text.to_owned())
    }
}

impl From<ExtensionId> for String {
    fn from(id: ExtensionId) -> Self {
        id.0
    }
}

impl AsRef<str> for ExtensionId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ExtensionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_that_could_leave_their_folder_are_refused() {
        for unsafe_id in [".", "..", ".hidden", "a..b", "..x", "x.."] {
            assert_eq!(
                unsafe_id.parse::<ExtensionId>(),
                Err(IdError::Dots),
                "{unsafe_id}"
            );
        }
        assert_eq!("a/b".parse::<ExtensionId>(), Err(IdError::Character));
        assert_eq!("".parse::<ExtensionId>(), Err(IdError::Empty));
    }

    #[test]
    fn plain_ids_with_single_dots_are_kept() -> Result<(), IdError> {
        let id: ExtensionId = "acme.docker-tools_2".parse()?;
        assert_eq!(
            id.folder_in(Path::new("/config/extensions")),
            PathBuf::from("/config/extensions/acme.docker-tools_2")
        );
        Ok(())
    }

    #[test]
    fn an_unsafe_id_is_refused_while_reading() {
        let read: Result<ExtensionId, _> = serde_json::from_str(r#""..""#);
        assert!(read.is_err());
    }
}
