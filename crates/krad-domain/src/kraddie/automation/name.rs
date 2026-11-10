use super::super::name::KraddieId;
use serde::{Deserialize, Serialize};
use std::fmt;

const MAX_AUTOMATION_NAME_CHARS: usize = 60;
const KEY_SEPARATOR: char = '/';

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AutomationId(pub u32);

impl AutomationId {
    pub const FIRST: Self = Self(1);

    #[must_use]
    pub fn after<'a>(taken: impl IntoIterator<Item = &'a Self>) -> Self {
        taken
            .into_iter()
            .map(|id| id.0)
            .max()
            .map_or(Self::FIRST, |last| Self(last.saturating_add(1)))
    }
}

impl fmt::Display for AutomationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutomationNameError {
    Empty,
    TooLong,
    Separator,
}

impl fmt::Display for AutomationNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("An automation needs a name"),
            Self::TooLong => write!(
                f,
                "An automation's name has at most {MAX_AUTOMATION_NAME_CHARS} characters"
            ),
            Self::Separator => f.write_str("An automation's name cannot contain a slash"),
        }
    }
}

impl std::error::Error for AutomationNameError {}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct AutomationName(String);

impl AutomationName {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn matches(&self, asked: &str) -> bool {
        self.0.eq_ignore_ascii_case(asked.trim())
    }
}

impl TryFrom<String> for AutomationName {
    type Error = AutomationNameError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        let name = text.trim();
        if name.is_empty() {
            return Err(AutomationNameError::Empty);
        }
        if name.chars().count() > MAX_AUTOMATION_NAME_CHARS {
            return Err(AutomationNameError::TooLong);
        }
        if name.contains(KEY_SEPARATOR) {
            return Err(AutomationNameError::Separator);
        }
        Ok(Self(name.to_owned()))
    }
}

impl std::str::FromStr for AutomationName {
    type Err = AutomationNameError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::try_from(text.to_owned())
    }
}

impl From<AutomationName> for String {
    fn from(name: AutomationName) -> Self {
        name.0
    }
}

impl fmt::Display for AutomationName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AutomationKey {
    pub kraddie: KraddieId,
    pub automation: AutomationId,
}

impl AutomationKey {
    #[must_use]
    pub const fn new(kraddie: KraddieId, automation: AutomationId) -> Self {
        Self {
            kraddie,
            automation,
        }
    }
}

impl fmt::Display for AutomationKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{KEY_SEPARATOR}{}", self.kraddie, self.automation)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutomationQuery<'a> {
    pub kraddie: Option<&'a str>,
    pub name: &'a str,
}

impl<'a> From<&'a str> for AutomationQuery<'a> {
    fn from(asked: &'a str) -> Self {
        match asked.split_once(KEY_SEPARATOR) {
            Some((kraddie, name)) => Self {
                kraddie: Some(kraddie.trim()),
                name: name.trim(),
            },
            None => Self {
                kraddie: None,
                name: asked.trim(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_trimmed_bounded_and_free_of_the_key_separator() {
        assert_eq!(
            " Nightly check "
                .parse::<AutomationName>()
                .map(String::from),
            Ok("Nightly check".to_owned())
        );
        assert_eq!(
            "   ".parse::<AutomationName>(),
            Err(AutomationNameError::Empty)
        );
        assert_eq!(
            "a/b".parse::<AutomationName>(),
            Err(AutomationNameError::Separator)
        );
        assert_eq!(
            "x".repeat(MAX_AUTOMATION_NAME_CHARS + 1)
                .parse::<AutomationName>(),
            Err(AutomationNameError::TooLong)
        );
    }

    #[test]
    fn ids_follow_the_highest_taken_and_queries_split_on_the_kraddie() {
        assert_eq!(AutomationId::after(&[]), AutomationId::FIRST);
        assert_eq!(
            AutomationId::after(&[AutomationId(3), AutomationId(1)]),
            AutomationId(4)
        );
        assert_eq!(
            AutomationQuery::from(" owl / Find bugs "),
            AutomationQuery {
                kraddie: Some("owl"),
                name: "Find bugs"
            }
        );
        assert_eq!(AutomationQuery::from("Find bugs").kraddie, None);
    }
}
