mod claude_token;
mod placement;
mod runner;
mod webhooks;

pub use claude_token::{ClaudeOauthToken, ClaudeOauthTokenError};
pub use placement::{CloudPlacement, CloudProcess, OpencodePassword};
pub use runner::*;
pub use webhooks::{RunnerWebhooks, SyncedWebhook, WebhookTarget};

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use crate::InstallId;

const MAX_ID_LEN: usize = 64;
const ID_SEPARATOR: char = '-';
const MIN_TOKEN_LEN: usize = 32;
const MAX_TOKEN_LEN: usize = 128;
const REDACTED: &str = "[REDACTED]";
const RUNNING: &str = "running";
const ENDED_PREFIX: &str = "ended:";
const UNKNOWN_CODE: &str = "?";

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CloudProcessId(String);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidCloudProcessId;

impl fmt::Display for InvalidCloudProcessId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a cloud process id is 1 to 64 lowercase letters, digits or '-'")
    }
}

impl std::error::Error for InvalidCloudProcessId {}

impl CloudProcessId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for CloudProcessId {
    type Err = InvalidCloudProcessId;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let valid = (1..=MAX_ID_LEN).contains(&raw.len())
            && !raw.starts_with(ID_SEPARATOR)
            && raw
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == ID_SEPARATOR);
        if valid {
            Ok(Self(raw.to_owned()))
        } else {
            Err(InvalidCloudProcessId)
        }
    }
}

impl TryFrom<String> for CloudProcessId {
    type Error = InvalidCloudProcessId;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        raw.parse()
    }
}

impl From<CloudProcessId> for String {
    fn from(id: CloudProcessId) -> Self {
        id.0
    }
}

impl fmt::Display for CloudProcessId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct JournalOffset(pub u64);

impl JournalOffset {
    #[must_use]
    pub const fn advanced(self, bytes: usize) -> Self {
        Self(self.0.saturating_add(bytes as u64))
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PairingToken(String);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidPairingToken;

impl fmt::Display for InvalidPairingToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a pairing token is 32 to 128 lowercase hexadecimal digits")
    }
}

impl std::error::Error for InvalidPairingToken {}

impl PairingToken {
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for PairingToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(REDACTED)
    }
}

impl FromStr for PairingToken {
    type Err = InvalidPairingToken;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let raw = raw.trim();
        let valid = (MIN_TOKEN_LEN..=MAX_TOKEN_LEN).contains(&raw.len())
            && raw
                .chars()
                .all(|c| c.is_ascii_digit() || matches!(c, 'a'..='f'));
        if valid {
            Ok(Self(raw.to_owned()))
        } else {
            Err(InvalidPairingToken)
        }
    }
}

impl TryFrom<String> for PairingToken {
    type Error = InvalidPairingToken;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        raw.parse()
    }
}

impl From<PairingToken> for String {
    fn from(token: PairingToken) -> Self {
        token.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pairing {
    pub install: InstallId,
    pub token: PairingToken,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CloudProcessRun {
    #[default]
    Running,
    Ended(Option<i32>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidCloudProcessRun;

impl fmt::Display for InvalidCloudProcessRun {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a cloud process state is 'running', 'ended:<code>' or 'ended:?'")
    }
}

impl std::error::Error for InvalidCloudProcessRun {}

impl fmt::Display for CloudProcessRun {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Running => formatter.write_str(RUNNING),
            Self::Ended(Some(code)) => write!(formatter, "{ENDED_PREFIX}{code}"),
            Self::Ended(None) => write!(formatter, "{ENDED_PREFIX}{UNKNOWN_CODE}"),
        }
    }
}

impl FromStr for CloudProcessRun {
    type Err = InvalidCloudProcessRun;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        if raw == RUNNING {
            return Ok(Self::Running);
        }
        let code = raw
            .strip_prefix(ENDED_PREFIX)
            .ok_or(InvalidCloudProcessRun)?;
        if code == UNKNOWN_CODE {
            return Ok(Self::Ended(None));
        }
        code.parse()
            .map(|code| Self::Ended(Some(code)))
            .map_err(|_| InvalidCloudProcessRun)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_ids_are_single_safe_path_segments() {
        assert!("chat-3f9c2a10".parse::<CloudProcessId>().is_ok());
        assert!("".parse::<CloudProcessId>().is_err());
        assert!("-flag".parse::<CloudProcessId>().is_err());
        assert!("../escape".parse::<CloudProcessId>().is_err());
        assert!("Chat".parse::<CloudProcessId>().is_err());
        assert!("a"
            .repeat(MAX_ID_LEN + 1)
            .parse::<CloudProcessId>()
            .is_err());
    }

    #[test]
    fn an_offset_advances_by_the_bytes_read() {
        assert_eq!(JournalOffset(10).advanced(5), JournalOffset(15));
        assert_eq!(JournalOffset(u64::MAX).advanced(1), JournalOffset(u64::MAX));
    }

    #[test]
    fn pairing_tokens_are_hexadecimal_and_never_printed() -> Result<(), InvalidPairingToken> {
        let token: PairingToken = "0123456789abcdef0123456789abcdef\n".parse()?;
        assert_eq!(token.expose(), "0123456789abcdef0123456789abcdef");
        assert_eq!(format!("{token:?}"), REDACTED);
        assert!("short".parse::<PairingToken>().is_err());
        assert!("0123456789ABCDEF0123456789ABCDEF"
            .parse::<PairingToken>()
            .is_err());
        Ok(())
    }

    #[test]
    fn a_kept_pairing_token_is_checked_when_read_back() -> serde_json::Result<()> {
        let token: PairingToken = serde_json::from_str(r#""0123456789abcdef0123456789abcdef""#)?;
        assert_eq!(
            serde_json::to_string(&token)?,
            r#""0123456789abcdef0123456789abcdef""#
        );
        assert!(serde_json::from_str::<PairingToken>(r#""not a token""#).is_err());
        Ok(())
    }

    #[test]
    fn a_process_state_survives_its_text_form() {
        for run in [
            CloudProcessRun::Running,
            CloudProcessRun::Ended(Some(-2)),
            CloudProcessRun::Ended(None),
        ] {
            assert_eq!(run.to_string().parse(), Ok(run));
        }
        assert_eq!(CloudProcessRun::Ended(None).to_string(), "ended:?");
        assert!("ended:x".parse::<CloudProcessRun>().is_err());
        assert!("stopped".parse::<CloudProcessRun>().is_err());
    }
}
