use serde::{Deserialize, Serialize};
use std::fmt;

pub use krad_domain::agents_api::*;

const REDACTED: &str = "[REDACTED]";

#[must_use]
pub fn current_api_info() -> ApiInfo {
    ApiInfo {
        api_version: API_VERSION,
        build: krad_host::HOST_CLI_BUILD.to_owned(),
        pid: Some(std::process::id()),
    }
}

#[derive(Clone, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BusToken(String);

impl PartialEq for BusToken {
    fn eq(&self, other: &Self) -> bool {
        crate::token::constant_time_eq(self.0.as_bytes(), other.0.as_bytes())
    }
}

impl BusToken {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for BusToken {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for BusToken {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl fmt::Debug for BusToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(REDACTED)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    #[serde(default)]
    pub api_version: ApiVersion,
    pub token: BusToken,
    pub from: Caller,
    pub request: Request,
}

#[cfg(test)]
mod tests;
