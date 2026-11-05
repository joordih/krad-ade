use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

pub const MAX_ACP_AGENT_ID_LEN: usize = 40;
const ID_PUNCTUATION: u8 = b'-';

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcpAgentIdError {
    Empty,
    TooLong,
    Character,
    Edge,
}

impl fmt::Display for AcpAgentIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "Enter an id",
            Self::TooLong => "Keep the id to 40 characters",
            Self::Character => "Use lowercase letters, digits and dashes",
            Self::Edge => "Start and end the id with a letter or a digit",
        })
    }
}

impl std::error::Error for AcpAgentIdError {}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct AcpAgentId {
    len: u8,
    bytes: [u8; MAX_ACP_AGENT_ID_LEN],
}

impl AcpAgentId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..usize::from(self.len)]).unwrap_or_default()
    }
}

const fn is_id_letter(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit()
}

impl FromStr for AcpAgentId {
    type Err = AcpAgentIdError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let text = text.trim().as_bytes();
        let (Some(&first), Some(&last)) = (text.first(), text.last()) else {
            return Err(AcpAgentIdError::Empty);
        };
        let len = u8::try_from(text.len())
            .ok()
            .filter(|len| usize::from(*len) <= MAX_ACP_AGENT_ID_LEN)
            .ok_or(AcpAgentIdError::TooLong)?;
        if !text
            .iter()
            .all(|&byte| is_id_letter(byte) || byte == ID_PUNCTUATION)
        {
            return Err(AcpAgentIdError::Character);
        }
        if !is_id_letter(first) || !is_id_letter(last) {
            return Err(AcpAgentIdError::Edge);
        }
        let mut bytes = [0; MAX_ACP_AGENT_ID_LEN];
        bytes[..text.len()].copy_from_slice(text);
        Ok(Self { len, bytes })
    }
}

impl TryFrom<String> for AcpAgentId {
    type Error = AcpAgentIdError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<AcpAgentId> for String {
    fn from(id: AcpAgentId) -> Self {
        id.as_str().to_owned()
    }
}

impl fmt::Display for AcpAgentId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl fmt::Debug for AcpAgentId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("AcpAgentId")
            .field(&self.as_str())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_lowercase_words_joined_by_dashes() {
        let id = " claude-code-acp ".parse::<AcpAgentId>();
        assert_eq!(
            id.map(|id| id.to_string()),
            Ok("claude-code-acp".to_owned())
        );
        assert_eq!("".parse::<AcpAgentId>(), Err(AcpAgentIdError::Empty));
        assert_eq!("  ".parse::<AcpAgentId>(), Err(AcpAgentIdError::Empty));
        assert_eq!(
            "Claude".parse::<AcpAgentId>(),
            Err(AcpAgentIdError::Character)
        );
        assert_eq!(
            "my agent".parse::<AcpAgentId>(),
            Err(AcpAgentIdError::Character)
        );
        assert_eq!("a:b".parse::<AcpAgentId>(), Err(AcpAgentIdError::Character));
        assert_eq!("-agent".parse::<AcpAgentId>(), Err(AcpAgentIdError::Edge));
        assert_eq!("agent-".parse::<AcpAgentId>(), Err(AcpAgentIdError::Edge));
        assert_eq!(
            "a".repeat(MAX_ACP_AGENT_ID_LEN + 1).parse::<AcpAgentId>(),
            Err(AcpAgentIdError::TooLong)
        );
        assert!("a"
            .repeat(MAX_ACP_AGENT_ID_LEN)
            .parse::<AcpAgentId>()
            .is_ok());
    }

    #[test]
    fn ids_persist_as_their_text_and_are_checked_when_read() -> serde_json::Result<()> {
        let id: AcpAgentId = serde_json::from_str(r#""codex-acp""#)?;
        assert_eq!(id.as_str(), "codex-acp");
        assert_eq!(serde_json::to_string(&id)?, r#""codex-acp""#);
        assert!(serde_json::from_str::<AcpAgentId>(r#""Codex ACP""#).is_err());
        Ok(())
    }
}
