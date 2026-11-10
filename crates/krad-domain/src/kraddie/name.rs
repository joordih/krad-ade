use crate::session_template::branch_slug;
use serde::{Deserialize, Serialize};
use std::fmt;

const MAX_KRADDIE_NAME_CHARS: usize = 40;
const ID_SEPARATOR: char = '-';

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KraddieNameError {
    Empty,
    TooLong,
    NoLetters,
}

impl fmt::Display for KraddieNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("A Kraddie needs a name"),
            Self::TooLong => write!(
                f,
                "A Kraddie's name has at most {MAX_KRADDIE_NAME_CHARS} characters"
            ),
            Self::NoLetters => f.write_str("A Kraddie's name needs a letter or a digit"),
        }
    }
}

impl std::error::Error for KraddieNameError {}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct KraddieName(String);

impl KraddieName {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn id(&self) -> KraddieId {
        KraddieId(slug_of(&self.0).unwrap_or_default())
    }

    #[must_use]
    pub fn matches(&self, asked: &str) -> bool {
        let asked = asked.trim();
        self.0.eq_ignore_ascii_case(asked) || self.id().as_str() == asked
    }
}

fn slug_of(text: &str) -> Option<String> {
    branch_slug(text).map(|slug| slug.as_str().to_owned())
}

impl TryFrom<String> for KraddieName {
    type Error = KraddieNameError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        let name = text.trim();
        if name.is_empty() {
            return Err(KraddieNameError::Empty);
        }
        if name.chars().count() > MAX_KRADDIE_NAME_CHARS {
            return Err(KraddieNameError::TooLong);
        }
        if slug_of(name).is_none() {
            return Err(KraddieNameError::NoLetters);
        }
        Ok(Self(name.to_owned()))
    }
}

impl std::str::FromStr for KraddieName {
    type Err = KraddieNameError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::try_from(text.to_owned())
    }
}

impl From<KraddieName> for String {
    fn from(name: KraddieName) -> Self {
        name.0
    }
}

impl fmt::Display for KraddieName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct KraddieId(String);

impl KraddieId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn numbered(&self, number: u64) -> String {
        format!("{}{ID_SEPARATOR}{number}", self.0)
    }
}

impl TryFrom<String> for KraddieId {
    type Error = KraddieNameError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        match slug_of(&text) {
            Some(slug) if slug == text => Ok(Self(text)),
            Some(_) | None => Err(KraddieNameError::NoLetters),
        }
    }
}

impl From<KraddieId> for String {
    fn from(id: KraddieId) -> Self {
        id.0
    }
}

impl fmt::Display for KraddieId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_trimmed_bounded_and_give_a_path_safe_id() {
        let name: KraddieName = "  Lint Kraddie ".parse().unwrap_or_else(|_| unreachable!());
        assert_eq!(name.as_str(), "Lint Kraddie");
        assert_eq!(name.id().as_str(), "lint-kraddie");
        assert!(name.matches("lint Kraddie"));
        assert!(name.matches("lint-kraddie"));
        assert_eq!("   ".parse::<KraddieName>(), Err(KraddieNameError::Empty));
        assert_eq!(
            "!!!".parse::<KraddieName>(),
            Err(KraddieNameError::NoLetters)
        );
        assert_eq!(
            "x".repeat(MAX_KRADDIE_NAME_CHARS + 1)
                .parse::<KraddieName>(),
            Err(KraddieNameError::TooLong)
        );
    }

    #[test]
    fn ids_refuse_anything_that_is_not_their_own_slug() {
        assert!(KraddieId::try_from("nightly-ci".to_owned()).is_ok());
        for bad in ["../etc", "Upper", "a b", ""] {
            assert!(KraddieId::try_from(bad.to_owned()).is_err(), "{bad}");
        }
        let id = KraddieId::try_from("nightly".to_owned()).unwrap_or_else(|_| unreachable!());
        assert_eq!(id.numbered(3), "nightly-3");
    }
}
