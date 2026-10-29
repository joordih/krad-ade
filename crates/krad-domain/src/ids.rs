use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct TerminalId(pub u64);

impl TerminalId {
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub struct TabId(pub u64);

impl TabId {
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct EnvironmentName(String);

impl EnvironmentName {
    #[must_use]
    pub fn new(raw: String) -> Self {
        Self(raw)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct GroupId(pub u64);

impl GroupId {
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct SessionId(pub u64);

impl SessionId {
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

const RESUME_ID_MAX_CHARS: usize = 128;
const RESUME_ID_PUNCTUATION: [char; 2] = ['-', '_'];

#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ResumeId(String);

impl ResumeId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct InvalidResumeId;

impl std::fmt::Display for InvalidResumeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid resume id")
    }
}

impl std::error::Error for InvalidResumeId {}

impl TryFrom<String> for ResumeId {
    type Error = InvalidResumeId;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        let valid = !raw.is_empty()
            && raw.chars().count() <= RESUME_ID_MAX_CHARS
            && raw
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || RESUME_ID_PUNCTUATION.contains(&ch));
        valid.then_some(Self(raw)).ok_or(InvalidResumeId)
    }
}

impl std::str::FromStr for ResumeId {
    type Err = InvalidResumeId;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::try_from(raw.to_owned())
    }
}

impl From<ResumeId> for String {
    fn from(id: ResumeId) -> Self {
        id.0
    }
}

impl std::fmt::Display for ResumeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_ids_only_take_shell_safe_characters() {
        let uuid = "0b7c3a52-9f1e-4d2a-8c6b-1f2e3d4c5b6a";
        assert_eq!(
            uuid.parse::<ResumeId>().map(String::from).as_deref(),
            Ok(uuid)
        );
        for bad in ["", "abc; rm -rf ~", "a b", "$(id)", "x'y"] {
            assert_eq!(bad.parse::<ResumeId>(), Err(InvalidResumeId), "{bad}");
        }
        assert!("a"
            .repeat(RESUME_ID_MAX_CHARS + 1)
            .parse::<ResumeId>()
            .is_err());
    }
}
