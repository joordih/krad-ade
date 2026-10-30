use std::fmt;
use std::str::FromStr;

use serde::Deserialize;

const TAG_PREFIX: char = 'v';
const SEPARATOR: char = '.';
const RUNNING_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(try_from = "String")]
pub struct AppVersion {
    major: u32,
    minor: u32,
    patch: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("`{0}` is not a stable release version")]
pub struct VersionError(String);

impl AppVersion {
    /// # Errors
    ///
    /// [`VersionError`] when the crate version is not `major.minor.patch`.
    pub fn running() -> Result<Self, VersionError> {
        RUNNING_VERSION.parse()
    }

    #[must_use]
    pub fn tag(self) -> String {
        format!("{TAG_PREFIX}{self}")
    }
}

impl FromStr for AppVersion {
    type Err = VersionError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let invalid = || VersionError(text.to_owned());
        let bare = text.strip_prefix(TAG_PREFIX).unwrap_or(text);
        let mut parts = bare.split(SEPARATOR).map(str::parse::<u32>);
        let mut next = || parts.next().and_then(Result::ok).ok_or_else(invalid);
        let version = Self {
            major: next()?,
            minor: next()?,
            patch: next()?,
        };
        if parts.next().is_some() {
            return Err(invalid());
        }
        Ok(version)
    }
}

impl TryFrom<String> for AppVersion {
    type Error = VersionError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl fmt::Display for AppVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn parses_bare_and_tagged_versions() -> TestResult {
        let bare: AppVersion = "0.1.5".parse()?;
        let tagged: AppVersion = "v0.1.5".parse()?;
        assert_eq!(bare, tagged);
        assert_eq!(bare.tag(), "v0.1.5");
        Ok(())
    }

    #[test]
    fn rejects_pre_releases_and_partial_versions() {
        for text in ["0.1", "0.1.5-beta", "0.1.5.1", "x.1.5", ""] {
            assert!(text.parse::<AppVersion>().is_err(), "{text} parsed");
        }
    }

    #[test]
    fn compares_numerically_not_lexically() -> TestResult {
        let older: AppVersion = "0.9.0".parse()?;
        let newer: AppVersion = "0.10.0".parse()?;
        assert!(newer > older);
        Ok(())
    }

    #[test]
    fn the_running_version_parses() -> TestResult {
        AppVersion::running()?;
        Ok(())
    }
}
