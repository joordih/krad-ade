use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

const VERSION_PREFIX: char = 'v';
const PART_SEPARATOR: char = '.';
const AT_LEAST: &str = ">=";
const CARET: char = '^';
const TILDE: char = '~';
const EQUALS: char = '=';

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ToolchainVersion {
    raw: String,
    parts: Vec<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotAVersion;

impl fmt::Display for NotAVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("The text holds no version number")
    }
}

impl std::error::Error for NotAVersion {}

impl ToolchainVersion {
    #[must_use]
    pub fn parts(&self) -> &[u64] {
        &self.parts
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    fn starts_with(&self, prefix: &[u64]) -> bool {
        self.parts.len() >= prefix.len() && self.parts[..prefix.len()] == *prefix
    }
}

fn leading_parts(text: &str) -> Vec<u64> {
    let digits_start = text.find(|c: char| c.is_ascii_digit());
    let Some(start) = digits_start else {
        return Vec::new();
    };
    let mut parts = Vec::new();
    for piece in text[start..].split(PART_SEPARATOR) {
        let digits: String = piece.chars().take_while(char::is_ascii_digit).collect();
        let Ok(number) = digits.parse::<u64>() else {
            break;
        };
        parts.push(number);
        if digits.len() != piece.len() {
            break;
        }
    }
    parts
}

impl FromStr for ToolchainVersion {
    type Err = NotAVersion;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let raw = text.trim();
        let parts = leading_parts(raw.trim_start_matches(VERSION_PREFIX));
        if parts.is_empty() {
            return Err(NotAVersion);
        }
        Ok(Self {
            raw: raw.to_owned(),
            parts,
        })
    }
}

impl TryFrom<String> for ToolchainVersion {
    type Error = NotAVersion;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        raw.parse()
    }
}

impl From<ToolchainVersion> for String {
    fn from(version: ToolchainVersion) -> Self {
        version.raw
    }
}

impl fmt::Display for ToolchainVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.raw)
    }
}

impl PartialEq for ToolchainVersion {
    fn eq(&self, other: &Self) -> bool {
        self.parts == other.parts
    }
}

impl Eq for ToolchainVersion {}

impl std::hash::Hash for ToolchainVersion {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.parts.hash(state);
    }
}

impl PartialOrd for ToolchainVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ToolchainVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        self.parts.cmp(&other.parts)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionRequirement {
    Prefix(Vec<u64>),
    AtLeast(Vec<u64>),
}

impl VersionRequirement {
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if let Some(rest) = text.strip_prefix(AT_LEAST) {
            return non_empty(leading_parts(rest)).map(Self::AtLeast);
        }
        let rest = text
            .trim_start_matches([CARET, TILDE, EQUALS])
            .trim_start_matches(VERSION_PREFIX);
        if !rest.starts_with(|c: char| c.is_ascii_digit()) {
            return None;
        }
        non_empty(leading_parts(rest)).map(Self::Prefix)
    }

    #[must_use]
    pub fn matches(&self, version: &ToolchainVersion) -> bool {
        match self {
            Self::Prefix(prefix) => version.starts_with(prefix),
            Self::AtLeast(minimum) => version.parts.as_slice() >= minimum.as_slice(),
        }
    }
}

fn non_empty(parts: Vec<u64>) -> Option<Vec<u64>> {
    (!parts.is_empty()).then_some(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn versions_read_their_leading_numbers() -> TestResult {
        let python: ToolchainVersion = "3.12.4".parse()?;
        assert_eq!(python.parts(), [3, 12, 4]);
        let node: ToolchainVersion = "v20.11.1".parse()?;
        assert_eq!(node.parts(), [20, 11, 1]);
        let rc: ToolchainVersion = "3.13.0rc2".parse()?;
        assert_eq!(rc.parts(), [3, 13, 0]);
        let named: ToolchainVersion = "temurin-21.0.2+13".parse()?;
        assert_eq!(named.parts(), [21, 0, 2]);
        assert!("stable".parse::<ToolchainVersion>().is_err());
        Ok(())
    }

    #[test]
    fn versions_order_numerically() -> TestResult {
        let old: ToolchainVersion = "3.9.18".parse()?;
        let new: ToolchainVersion = "3.12.1".parse()?;
        assert!(new > old);
        Ok(())
    }

    #[test]
    fn requirements_from_hint_files() -> TestResult {
        let exact = VersionRequirement::parse("3.12").ok_or("no requirement")?;
        assert!(exact.matches(&"3.12.4".parse()?));
        assert!(!exact.matches(&"3.13.0".parse()?));
        let node = VersionRequirement::parse("v20").ok_or("no requirement")?;
        assert!(node.matches(&"20.11.1".parse()?));
        let minimum = VersionRequirement::parse(">=18.2").ok_or("no requirement")?;
        assert!(minimum.matches(&"20.0.0".parse()?));
        assert!(!minimum.matches(&"18.1.9".parse()?));
        assert_eq!(VersionRequirement::parse("lts/*"), None);
        assert_eq!(VersionRequirement::parse("system"), None);
        Ok(())
    }
}
