use super::digest::{ChatDigest, DigestBook};
use serde::de::IgnoredAny;
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;

pub const DIGESTS_FORMAT_VERSION: DigestsFormatVersion = DigestsFormatVersion(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DigestsFormatVersion(pub u32);

impl Default for DigestsFormatVersion {
    fn default() -> Self {
        DIGESTS_FORMAT_VERSION
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct DigestsFileError(String);

impl fmt::Display for DigestsFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Not a chat digests file: {}", self.0)
    }
}

impl std::error::Error for DigestsFileError {}

#[derive(Deserialize)]
#[serde(untagged)]
enum Lenient {
    Digest(ChatDigest),
    Unreadable(IgnoredAny),
}

fn readable_digests<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<ChatDigest>, D::Error> {
    let entries = Vec::<Lenient>::deserialize(deserializer)?;
    Ok(entries
        .into_iter()
        .filter_map(|entry| match entry {
            Lenient::Digest(digest) => Some(digest),
            Lenient::Unreadable(IgnoredAny) => None,
        })
        .collect())
}

#[derive(Serialize, Deserialize)]
struct DigestsFile {
    #[serde(default)]
    format: DigestsFormatVersion,
    #[serde(default, deserialize_with = "readable_digests")]
    digests: Vec<ChatDigest>,
}

/// # Errors
/// Fails when the text is not a JSON object of digests.
pub fn read_digests(text: &str) -> Result<DigestBook, DigestsFileError> {
    let file: DigestsFile =
        serde_json::from_str(text).map_err(|error| DigestsFileError(error.to_string()))?;
    Ok(DigestBook::new(file.digests))
}

/// # Errors
/// Fails when the digests cannot be written as JSON.
pub fn write_digests(book: &DigestBook) -> Result<String, DigestsFileError> {
    let file = DigestsFile {
        format: DIGESTS_FORMAT_VERSION,
        digests: book.digests().to_vec(),
    };
    serde_json::to_string_pretty(&file).map_err(|error| DigestsFileError(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::super::digest::{DigestChat, DigestDraft};
    use super::*;
    use crate::ids::{EnvironmentName, SessionId, TerminalId};
    use std::time::{Duration, SystemTime};

    fn book() -> DigestBook {
        let mut book = DigestBook::default();
        book.record(DigestDraft {
            chat: DigestChat {
                environment: EnvironmentName::new("main".to_owned()),
                session: SessionId(4),
                terminal: TerminalId(9),
                title: "Login flow".to_owned(),
                folder: Some(std::path::PathBuf::from("/work/api")),
                resume: None,
            },
            facts: vec!["Sessions expire after 30 minutes".to_owned()],
            covered: 6,
            written: SystemTime::UNIX_EPOCH + Duration::from_secs(60),
        });
        book
    }

    #[test]
    fn digests_round_trip_with_their_format() {
        let text = write_digests(&book()).unwrap_or_default();
        assert!(text.contains("\"format\": 1"), "{text}");
        assert_eq!(read_digests(&text), Ok(book()));
    }

    #[test]
    fn unreadable_entries_and_newer_fields_are_skipped_not_fatal() {
        let good = write_digests(&book()).unwrap_or_default();
        let value: serde_json::Value = serde_json::from_str(&good).unwrap_or_default();
        let mut digest = value["digests"][0].clone();
        digest["mood"] = serde_json::json!("new field from a later app");
        let text = serde_json::json!({
            "format": 7,
            "digests": [digest, {"id": "broken"}, 42],
        })
        .to_string();
        let read = read_digests(&text).unwrap_or_default();
        assert_eq!(read.len(), 1);
        assert_eq!(read_digests("{}"), Ok(DigestBook::default()));
        assert!(read_digests("not json").is_err());
    }
}
