use serde::{Deserialize, Serialize};
use std::fmt;

const REDACTED: &str = "[REDACTED]";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitlabSettings {
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub sign_in: GitlabSignIn,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitlabSignIn {
    #[default]
    Browser,
    DeviceCode,
}

impl GitlabSignIn {
    pub const ALL: [Self; 2] = [Self::Browser, Self::DeviceCode];
}

impl GitlabSettings {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.host.trim().is_empty() && self.client_id.trim().is_empty()
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitlabTokens {
    pub access_token: String,
    pub refresh_token: String,
}

impl fmt::Debug for GitlabTokens {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GitlabTokens")
            .field("access_token", &REDACTED)
            .field("refresh_token", &REDACTED)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_shows_the_tokens() {
        let tokens = GitlabTokens {
            access_token: "access-secret".to_owned(),
            refresh_token: "refresh-secret".to_owned(),
        };
        let printed = format!("{tokens:?}");
        assert!(!printed.contains("secret"));
        assert!(printed.contains(REDACTED));
    }
}
