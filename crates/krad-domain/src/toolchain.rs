use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ToolchainId(String);

impl ToolchainId {
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into().to_ascii_lowercase())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ToolchainId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ToolchainPaths(BTreeMap<ToolchainId, PathBuf>);

impl ToolchainPaths {
    #[must_use]
    pub fn get(&self, toolchain: &ToolchainId) -> Option<&Path> {
        self.0.get(toolchain).map(PathBuf::as_path)
    }

    pub fn choose(&mut self, toolchain: ToolchainId, folder: PathBuf) {
        self.0.insert(toolchain, folder);
    }

    pub fn forget(&mut self, toolchain: &ToolchainId) {
        self.0.remove(toolchain);
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionSource {
    Git(String),
    Folder(PathBuf),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionEntry {
    pub id: String,
    pub source: ExtensionSource,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
}

const fn enabled_by_default() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chosen_folders_persist_by_toolchain_id() -> serde_json::Result<()> {
        let mut paths = ToolchainPaths::default();
        paths.choose(ToolchainId::new("Java"), PathBuf::from("/opt/jdk-25"));
        let text = serde_json::to_string(&paths)?;
        assert_eq!(text, r#"{"java":"/opt/jdk-25"}"#);
        let mut back: ToolchainPaths = serde_json::from_str(&text)?;
        assert_eq!(
            back.get(&ToolchainId::new("java")),
            Some(Path::new("/opt/jdk-25"))
        );
        back.forget(&ToolchainId::new("java"));
        assert!(back.is_empty());
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
