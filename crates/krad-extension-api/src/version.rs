use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

const SEPARATOR: char = '.';

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct ApiVersion {
    pub major: u32,
    pub minor: u32,
}

pub const CURRENT_API: ApiVersion = ApiVersion { major: 1, minor: 0 };

impl ApiVersion {
    #[must_use]
    pub const fn new(major: u32, minor: u32) -> Self {
        Self { major, minor }
    }

    #[must_use]
    pub const fn supports(self, requested: Self) -> bool {
        self.major == requested.major && self.minor >= requested.minor
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("an API version reads as major.minor, such as 1.0")]
pub struct ApiVersionError;

impl FromStr for ApiVersion {
    type Err = ApiVersionError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let (major, minor) = text.split_once(SEPARATOR).ok_or(ApiVersionError)?;
        Ok(Self {
            major: major.parse().map_err(|_| ApiVersionError)?,
            minor: minor.parse().map_err(|_| ApiVersionError)?,
        })
    }
}

impl TryFrom<String> for ApiVersion {
    type Error = ApiVersionError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<ApiVersion> for String {
    fn from(version: ApiVersion) -> Self {
        version.to_string()
    }
}

impl fmt::Display for ApiVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{SEPARATOR}{}", self.major, self.minor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_app_supports_older_minors_of_its_own_major_only() {
        let app = ApiVersion::new(1, 3);
        assert!(app.supports(ApiVersion::new(1, 0)));
        assert!(app.supports(ApiVersion::new(1, 3)));
        assert!(!app.supports(ApiVersion::new(1, 4)));
        assert!(!app.supports(ApiVersion::new(2, 0)));
    }

    #[test]
    fn versions_read_and_write_as_major_dot_minor() {
        assert_eq!("1.2".parse(), Ok(ApiVersion::new(1, 2)));
        assert_eq!(ApiVersion::new(1, 2).to_string(), "1.2");
        assert_eq!("1".parse::<ApiVersion>(), Err(ApiVersionError));
        assert_eq!("1.x".parse::<ApiVersion>(), Err(ApiVersionError));
    }
}
