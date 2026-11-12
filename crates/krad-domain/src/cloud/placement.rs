use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

use super::{CloudProcessId, CloudRunnerId, JournalOffset, REDACTED};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudPlacement {
    pub runner: CloudRunnerId,
    pub repo_dir: PathBuf,
    pub worktree: PathBuf,
    pub branch: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudProcess {
    pub id: CloudProcessId,
    pub offset: JournalOffset,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OpencodePassword(String);

impl OpencodePassword {
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl From<String> for OpencodePassword {
    fn from(password: String) -> Self {
        Self(password)
    }
}

impl fmt::Debug for OpencodePassword {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(REDACTED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_opencode_password_is_never_printed() -> serde_json::Result<()> {
        let password = OpencodePassword::from("hunter2".to_owned());
        assert_eq!(format!("{password:?}"), REDACTED);
        assert_eq!(serde_json::to_string(&password)?, r#""hunter2""#);
        Ok(())
    }
}
