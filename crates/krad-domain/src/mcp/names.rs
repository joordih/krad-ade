use crate::http_url::{checked_http_url, HttpUrlError};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

const NAME_PUNCTUATION: &str = "-_.";
const VARIABLE_FORBIDDEN: &str = "=:";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum McpNameError {
    Empty,
    Character,
}

impl fmt::Display for McpNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "Enter a name",
            Self::Character => "Use letters, digits, dashes, dots and underscores",
        })
    }
}

impl std::error::Error for McpNameError {}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct McpServerName(String);

impl McpServerName {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for McpServerName {
    type Err = McpNameError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let text = text.trim();
        if text.is_empty() {
            return Err(McpNameError::Empty);
        }
        let allowed = |c: char| c.is_ascii_alphanumeric() || NAME_PUNCTUATION.contains(c);
        if !text.chars().all(allowed) {
            return Err(McpNameError::Character);
        }
        Ok(Self(text.to_owned()))
    }
}

impl TryFrom<String> for McpServerName {
    type Error = McpNameError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<McpServerName> for String {
    fn from(name: McpServerName) -> Self {
        name.0
    }
}

impl fmt::Display for McpServerName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct McpVariableName(String);

impl McpVariableName {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for McpVariableName {
    type Err = McpNameError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let text = text.trim();
        if text.is_empty() {
            return Err(McpNameError::Empty);
        }
        let allowed = |c: char| c.is_ascii_graphic() && !VARIABLE_FORBIDDEN.contains(c);
        if !text.chars().all(allowed) {
            return Err(McpNameError::Character);
        }
        Ok(Self(text.to_owned()))
    }
}

impl TryFrom<String> for McpVariableName {
    type Error = McpNameError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<McpVariableName> for String {
    fn from(name: McpVariableName) -> Self {
        name.0
    }
}

impl fmt::Display for McpVariableName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct McpUrl(String);

impl McpUrl {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for McpUrl {
    type Err = HttpUrlError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Ok(Self(checked_http_url(text)?.to_owned()))
    }
}

impl TryFrom<String> for McpUrl {
    type Error = HttpUrlError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<McpUrl> for String {
    fn from(url: McpUrl) -> Self {
        url.0
    }
}

impl fmt::Display for McpUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_names_keep_to_a_safe_alphabet() {
        assert_eq!(
            " my-server_1.0 ".parse::<McpServerName>().map(String::from),
            Ok("my-server_1.0".to_owned())
        );
        assert_eq!("".parse::<McpServerName>(), Err(McpNameError::Empty));
        assert_eq!(
            "two words".parse::<McpServerName>(),
            Err(McpNameError::Character)
        );
        assert_eq!("a/b".parse::<McpServerName>(), Err(McpNameError::Character));
    }

    #[test]
    fn variable_names_refuse_separators_and_spaces() {
        assert!("X-Api-Key".parse::<McpVariableName>().is_ok());
        assert!("GITHUB_TOKEN".parse::<McpVariableName>().is_ok());
        assert_eq!(
            "A=B".parse::<McpVariableName>(),
            Err(McpNameError::Character)
        );
        assert_eq!(
            "Bad Name".parse::<McpVariableName>(),
            Err(McpNameError::Character)
        );
    }
}
