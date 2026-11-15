use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

use crate::VAULT_DIR;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum VaultEntryKind {
    Folder,
    Note,
    Attachment,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct VaultEntry {
    pub path: String,
    pub kind: VaultEntryKind,
}

#[must_use]
pub fn vault_dir(root: &Path) -> PathBuf {
    root.join(VAULT_DIR)
}

#[must_use]
pub fn vault_path(root: &Path, path: &str) -> Option<PathBuf> {
    let mut full = vault_dir(root);
    for part in Path::new(path).components() {
        match part {
            Component::Normal(part) => full.push(part),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(full)
}
