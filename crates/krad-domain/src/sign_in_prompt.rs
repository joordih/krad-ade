use serde::{Deserialize, Serialize};

use crate::host::SshDestination;

const CONFIRM_MARKER: &str = "(yes/no";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptKind {
    Secret,
    Confirm,
}

impl PromptKind {
    #[must_use]
    pub fn of(text: &str) -> Self {
        if text.contains(CONFIRM_MARKER) {
            Self::Confirm
        } else {
            Self::Secret
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "asker", rename_all = "snake_case")]
pub enum PromptAsker {
    Host { destination: SshDestination },
    Git,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_key_questions_are_confirmations_and_the_rest_secrets() {
        assert_eq!(
            PromptKind::of("Are you sure you want to continue connecting (yes/no/[fingerprint])? "),
            PromptKind::Confirm
        );
        assert_eq!(PromptKind::of("me@devbox's password: "), PromptKind::Secret);
    }
}
