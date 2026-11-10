use serde::{Deserialize, Serialize};
use std::fmt;

const VERSION_PREFIX: &str = "v";
const UNSUPPORTED_VERSION: &str = "This app serves the agents API";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ApiVersion(pub u16);

pub const API_VERSION: ApiVersion = ApiVersion(1);
const FIRST_API_VERSION: ApiVersion = ApiVersion(1);

impl Default for ApiVersion {
    fn default() -> Self {
        FIRST_API_VERSION
    }
}

impl fmt::Display for ApiVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{VERSION_PREFIX}{}", self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiInfo {
    pub api_version: ApiVersion,
    pub build: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
}

impl ApiInfo {
    #[must_use]
    pub fn current() -> Self {
        Self {
            api_version: API_VERSION,
            build: krad_host::HOST_CLI_BUILD.to_owned(),
            pid: Some(std::process::id()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Unauthorized,
    UnsupportedApiVersion,
    BadRequest,
    NotFound,
    Forbidden,
    Unsupported,
    Unavailable,
    Failed,
    NotOwned,
    Busy,
    WorktreeNotClean,
    SlowSubscriber,
    OutOfReach,
}

impl ErrorCode {
    pub const ALL: [Self; 13] = [
        Self::Unauthorized,
        Self::UnsupportedApiVersion,
        Self::BadRequest,
        Self::NotFound,
        Self::Forbidden,
        Self::Unsupported,
        Self::Unavailable,
        Self::Failed,
        Self::NotOwned,
        Self::Busy,
        Self::WorktreeNotClean,
        Self::SlowSubscriber,
        Self::OutOfReach,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Unauthorized => "unauthorized",
            Self::UnsupportedApiVersion => "unsupported_api_version",
            Self::BadRequest => "bad_request",
            Self::NotFound => "not_found",
            Self::Forbidden => "forbidden",
            Self::Unsupported => "unsupported",
            Self::Unavailable => "unavailable",
            Self::Failed => "failed",
            Self::NotOwned => "not_owned",
            Self::Busy => "busy",
            Self::WorktreeNotClean => "worktree_not_clean",
            Self::SlowSubscriber => "slow_subscriber",
            Self::OutOfReach => "out_of_reach",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{message}")]
pub struct ApiError {
    pub code: ErrorCode,
    pub message: String,
}

impl ApiError {
    #[must_use]
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    #[must_use]
    pub fn unsupported_version(asked: ApiVersion) -> Self {
        Self::new(
            ErrorCode::UnsupportedApiVersion,
            format!("{UNSUPPORTED_VERSION} {API_VERSION}, not {asked}"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_keep_their_names_on_the_wire() -> Result<(), serde_json::Error> {
        for code in ErrorCode::ALL {
            assert_eq!(serde_json::to_string(&code)?, format!("\"{code}\""));
            assert_eq!(
                serde_json::from_str::<ErrorCode>(&format!("\"{code}\""))?,
                code
            );
        }
        let error = ApiError::new(ErrorCode::NotFound, "No session 9");
        let line = r#"{"code":"not_found","message":"No session 9"}"#;
        assert_eq!(serde_json::to_string(&error)?, line);
        assert_eq!(serde_json::from_str::<ApiError>(line)?, error);
        Ok(())
    }

    #[test]
    fn a_caller_without_a_version_speaks_the_first_one() {
        assert_eq!(ApiVersion::default(), ApiVersion(1));
        assert_eq!(API_VERSION.to_string(), "v1");
        let refused = ApiError::unsupported_version(ApiVersion(2));
        assert_eq!(refused.code, ErrorCode::UnsupportedApiVersion);
        assert_eq!(refused.message, "This app serves the agents API v1, not v2");
    }
}
