use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

const MIN_LEN: usize = 8;
const MAX_LEN: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct InstallId(String);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidInstallId;

impl fmt::Display for InvalidInstallId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an installation id is 8 to 64 lowercase letters, digits or '-'")
    }
}

impl std::error::Error for InvalidInstallId {}

impl InstallId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for InstallId {
    type Err = InvalidInstallId;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let raw = raw.trim();
        let valid = (MIN_LEN..=MAX_LEN).contains(&raw.len())
            && raw
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if valid {
            Ok(Self(raw.to_owned()))
        } else {
            Err(InvalidInstallId)
        }
    }
}

impl TryFrom<String> for InstallId {
    type Error = InvalidInstallId;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        raw.parse()
    }
}

impl From<InstallId> for String {
    fn from(id: InstallId) -> Self {
        id.0
    }
}

impl fmt::Display for InstallId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_plain_lowercase_words_safe_in_paths() {
        assert!("3f9c2a10b7e44d21".parse::<InstallId>().is_ok());
        assert!("short".parse::<InstallId>().is_err());
        assert!("../../etc/passwd".parse::<InstallId>().is_err());
        assert!("Upper-Case-Id".parse::<InstallId>().is_err());
    }
}
