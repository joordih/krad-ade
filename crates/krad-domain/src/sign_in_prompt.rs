use serde::{Deserialize, Serialize};

use crate::cloud::CloudRunnerId;
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
    Host {
        destination: SshDestination,
    },
    Git,
    Runner {
        runner: CloudRunnerId,
    },
    #[serde(other)]
    Unknown,
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

    #[test]
    fn a_runner_asks_by_its_id_and_a_newer_asker_reads_as_unknown() -> serde_json::Result<()> {
        let runner = PromptAsker::Runner {
            runner: CloudRunnerId(4),
        };
        let json = serde_json::to_string(&runner)?;
        assert_eq!(json, r#"{"asker":"runner","runner":4}"#);
        assert_eq!(serde_json::from_str::<PromptAsker>(&json)?, runner);
        assert_eq!(
            serde_json::from_str::<PromptAsker>(r#"{"asker":"extension","id":"x"}"#)?,
            PromptAsker::Unknown
        );
        Ok(())
    }
}
