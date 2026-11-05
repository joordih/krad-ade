use crate::extension_id::ExtensionId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ServerRuntimeId(String);

impl ServerRuntimeId {
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into().to_ascii_lowercase())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ServerRuntimeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ServerRuntimePaths(BTreeMap<ServerRuntimeId, PathBuf>);

impl ServerRuntimePaths {
    #[must_use]
    pub fn get(&self, server_runtime: &ServerRuntimeId) -> Option<&Path> {
        self.0.get(server_runtime).map(PathBuf::as_path)
    }

    pub fn choose(&mut self, server_runtime: ServerRuntimeId, folder: PathBuf) {
        self.0.insert(server_runtime, folder);
    }

    pub fn forget(&mut self, server_runtime: &ServerRuntimeId) {
        self.0.remove(server_runtime);
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

const COMMIT_HEX_LEN: usize = 40;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct GitCommit(String);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GitCommitError;

impl fmt::Display for GitCommitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "a commit is written as {COMMIT_HEX_LEN} hexadecimal digits"
        )
    }
}

impl std::error::Error for GitCommitError {}

impl TryFrom<String> for GitCommit {
    type Error = GitCommitError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        let hex = text.trim().to_ascii_lowercase();
        if hex.len() == COMMIT_HEX_LEN && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            Ok(Self(hex))
        } else {
            Err(GitCommitError)
        }
    }
}

impl std::str::FromStr for GitCommit {
    type Err = GitCommitError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::try_from(text.to_owned())
    }
}

impl From<GitCommit> for String {
    fn from(commit: GitCommit) -> Self {
        commit.0
    }
}

impl AsRef<str> for GitCommit {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for GitCommit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionSource {
    Git(String),
    Pinned { url: String, commit: GitCommit },
    Folder(PathBuf),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExtensionFingerprint(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionEntry {
    pub id: ExtensionId,
    pub source: ExtensionSource,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trusted: Option<ExtensionFingerprint>,
}

const fn enabled_by_default() -> bool {
    true
}

pub(crate) fn lenient_extensions<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<ExtensionEntry>, D::Error> {
    let entries = Vec::<serde_json::Value>::deserialize(deserializer)?;
    Ok(entries
        .into_iter()
        .filter_map(|entry| serde_json::from_value(entry).ok())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chosen_folders_persist_by_server_runtime_id() -> serde_json::Result<()> {
        let mut paths = ServerRuntimePaths::default();
        paths.choose(ServerRuntimeId::new("Java"), PathBuf::from("/opt/jdk-25"));
        let text = serde_json::to_string(&paths)?;
        assert_eq!(text, r#"{"java":"/opt/jdk-25"}"#);
        let mut back: ServerRuntimePaths = serde_json::from_str(&text)?;
        assert_eq!(
            back.get(&ServerRuntimeId::new("java")),
            Some(Path::new("/opt/jdk-25"))
        );
        back.forget(&ServerRuntimeId::new("java"));
        assert!(back.is_empty());
        Ok(())
    }

    #[test]
    fn commits_are_forty_hex_digits_kept_lowercase() {
        let upper = "ABCDEF0123456789abcdef0123456789ABCDEF01";
        assert_eq!(
            upper.parse::<GitCommit>().map(String::from),
            Ok(upper.to_ascii_lowercase())
        );
        assert_eq!("abc".parse::<GitCommit>(), Err(GitCommitError));
        assert_eq!(
            "g".repeat(COMMIT_HEX_LEN).parse::<GitCommit>(),
            Err(GitCommitError)
        );
    }

    #[test]
    fn a_pinned_extension_reads_back_and_older_git_entries_still_read() -> serde_json::Result<()> {
        let pinned = ExtensionSource::Pinned {
            url: "https://github.com/acme/krad-docker".to_owned(),
            commit: GitCommit("0".repeat(COMMIT_HEX_LEN)),
        };
        let text = serde_json::to_string(&pinned)?;
        assert_eq!(serde_json::from_str::<ExtensionSource>(&text)?, pinned);
        let older: ExtensionSource = serde_json::from_str(r#"{"git":"https://x/zig.git"}"#)?;
        assert_eq!(older, ExtensionSource::Git("https://x/zig.git".to_owned()));
        Ok(())
    }

    #[test]
    fn an_extension_is_enabled_unless_said_otherwise() -> serde_json::Result<()> {
        let entry: ExtensionEntry =
            serde_json::from_str(r#"{"id":"zig","source":{"git":"https://example.com/zig.git"}}"#)?;
        assert!(entry.enabled);
        Ok(())
    }
}
