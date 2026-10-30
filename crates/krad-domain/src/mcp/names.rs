use crate::gateway::{host_of, is_loopback};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

const NAME_PUNCTUATION: &str = "-_.";
const VARIABLE_FORBIDDEN: &str = "=:";
const HTTPS_SCHEME: &str = "https://";
const HTTP_SCHEME: &str = "http://";
const PATH_SEPARATOR: char = '/';
const USERINFO_SEPARATOR: char = '@';

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum McpUrlError {
    Empty,
    NoScheme,
    NoHost,
    PlainHttpToRemote,
    Credentials,
}

impl fmt::Display for McpUrlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "Enter the server address",
            Self::NoScheme => "The address starts with http:// or https://",
            Self::NoHost => "The address has no host",
            Self::PlainHttpToRemote => {
                "Use https:// for a server on another machine; plain http:// is only for this one"
            }
            Self::Credentials => "Put credentials in a header, not in the address",
        })
    }
}

impl std::error::Error for McpUrlError {}

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
    type Err = McpUrlError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let text = text.trim();
        if text.is_empty() {
            return Err(McpUrlError::Empty);
        }
        let lowered = text.to_ascii_lowercase();
        let (secure, rest) = if lowered.starts_with(HTTPS_SCHEME) {
            (true, &text[HTTPS_SCHEME.len()..])
        } else if lowered.starts_with(HTTP_SCHEME) {
            (false, &text[HTTP_SCHEME.len()..])
        } else {
            return Err(McpUrlError::NoScheme);
        };
        if rest.chars().any(char::is_whitespace) {
            return Err(McpUrlError::NoHost);
        }
        let authority = rest
            .split_once(PATH_SEPARATOR)
            .map_or(rest, |(authority, _)| authority);
        if authority.contains(USERINFO_SEPARATOR) {
            return Err(McpUrlError::Credentials);
        }
        let host = host_of(authority);
        if host.is_empty() {
            return Err(McpUrlError::NoHost);
        }
        if !secure && !is_loopback(host) {
            return Err(McpUrlError::PlainHttpToRemote);
        }
        Ok(Self(text.to_owned()))
    }
}

impl TryFrom<String> for McpUrl {
    type Error = McpUrlError;

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

    #[test]
    fn addresses_keep_their_path_and_plain_http_stays_local() {
        assert_eq!(
            "https://mcp.example.test/mcp/"
                .parse::<McpUrl>()
                .map(String::from),
            Ok("https://mcp.example.test/mcp/".to_owned())
        );
        assert!("http://127.0.0.1:3000/mcp".parse::<McpUrl>().is_ok());
        assert_eq!(
            "http://10.0.0.4/mcp".parse::<McpUrl>(),
            Err(McpUrlError::PlainHttpToRemote)
        );
        assert_eq!(
            "https://user:pass@example.test".parse::<McpUrl>(),
            Err(McpUrlError::Credentials)
        );
        assert_eq!("example.test".parse::<McpUrl>(), Err(McpUrlError::NoScheme));
    }
}
