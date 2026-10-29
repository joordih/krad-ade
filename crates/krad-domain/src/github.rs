use serde::{Deserialize, Serialize};
use std::fmt;

const REDACTED: &str = "[REDACTED]";

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GithubToken {
    pub access_token: String,
}

impl fmt::Debug for GithubToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GithubToken")
            .field("access_token", &REDACTED)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_shows_the_token() {
        let token = GithubToken {
            access_token: "gho_secret".to_owned(),
        };
        let printed = format!("{token:?}");
        assert!(!printed.contains("secret"));
        assert!(printed.contains(REDACTED));
    }
}
