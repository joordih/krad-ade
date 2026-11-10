use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

const SHA1_HEX_LEN: usize = 40;
const SHA256_HEX_LEN: usize = 64;
const NOT_AN_OBJECT_ID: &str = "not a git object id";

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct GitObjectId(String);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectIdError;

impl fmt::Display for ObjectIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(NOT_AN_OBJECT_ID)
    }
}

impl std::error::Error for ObjectIdError {}

impl GitObjectId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for GitObjectId {
    type Err = ObjectIdError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let raw = raw.trim();
        let hex = raw
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        if hex && matches!(raw.len(), SHA1_HEX_LEN | SHA256_HEX_LEN) {
            Ok(Self(raw.to_owned()))
        } else {
            Err(ObjectIdError)
        }
    }
}

impl TryFrom<String> for GitObjectId {
    type Error = ObjectIdError;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        raw.parse()
    }
}

impl From<GitObjectId> for String {
    fn from(id: GitObjectId) -> Self {
        id.0
    }
}

impl fmt::Display for GitObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OBJECT: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

    #[test]
    fn an_object_id_is_a_full_lowercase_object_id() {
        assert_eq!(
            OBJECT.parse::<GitObjectId>().map(String::from).as_deref(),
            Ok(OBJECT)
        );
        assert_eq!("4b825dc".parse::<GitObjectId>(), Err(ObjectIdError));
        assert_eq!(
            OBJECT.to_uppercase().parse::<GitObjectId>(),
            Err(ObjectIdError)
        );
        assert!(serde_json::from_str::<GitObjectId>("\"HEAD\"").is_err());
    }
}
