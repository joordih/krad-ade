use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use super::REDACTED;

const MAX_TOKEN_LEN: usize = 512;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ClaudeOauthToken(String);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaudeOauthTokenError {
    Empty,
    Malformed,
}

impl fmt::Display for ClaudeOauthTokenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("Paste the token claude setup-token printed"),
            Self::Malformed => formatter.write_str(
                "A Claude Code token is one word of printable characters, without spaces",
            ),
        }
    }
}

impl std::error::Error for ClaudeOauthTokenError {}

impl ClaudeOauthToken {
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ClaudeOauthToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(REDACTED)
    }
}

impl FromStr for ClaudeOauthToken {
    type Err = ClaudeOauthTokenError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err(ClaudeOauthTokenError::Empty);
        }
        let valid = raw.len() <= MAX_TOKEN_LEN && raw.chars().all(|c| c.is_ascii_graphic());
        if valid {
            Ok(Self(raw.to_owned()))
        } else {
            Err(ClaudeOauthTokenError::Malformed)
        }
    }
}

impl TryFrom<String> for ClaudeOauthToken {
    type Error = ClaudeOauthTokenError;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        raw.parse()
    }
}

impl From<ClaudeOauthToken> for String {
    fn from(token: ClaudeOauthToken) -> Self {
        token.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pasted_token_is_trimmed_checked_and_never_printed() -> Result<(), ClaudeOauthTokenError> {
        let token: ClaudeOauthToken = "  sk-ant-oat01-abc_DEF\n".parse()?;
        assert_eq!(token.expose(), "sk-ant-oat01-abc_DEF");
        assert_eq!(format!("{token:?}"), REDACTED);
        assert_eq!(
            " ".parse::<ClaudeOauthToken>(),
            Err(ClaudeOauthTokenError::Empty)
        );
        assert_eq!(
            "two words".parse::<ClaudeOauthToken>(),
            Err(ClaudeOauthTokenError::Malformed)
        );
        Ok(())
    }

    #[test]
    fn a_kept_token_is_checked_when_read_back() -> serde_json::Result<()> {
        let token: ClaudeOauthToken = serde_json::from_str(r#""sk-ant-oat01-x""#)?;
        assert_eq!(serde_json::to_string(&token)?, r#""sk-ant-oat01-x""#);
        assert!(serde_json::from_str::<ClaudeOauthToken>(r#""a b""#).is_err());
        Ok(())
    }
}
