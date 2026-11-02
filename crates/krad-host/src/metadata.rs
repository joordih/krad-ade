use std::fs;
use std::time::SystemTime;

use crate::sftp::FileKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Metadata {
    pub kind: FileKind,
    pub len: u64,
    pub modified: Option<SystemTime>,
}

impl Metadata {
    #[must_use]
    pub fn is_dir(&self) -> bool {
        self.kind == FileKind::Directory
    }
}

impl From<fs::Metadata> for Metadata {
    fn from(metadata: fs::Metadata) -> Self {
        let kind = if metadata.is_dir() {
            FileKind::Directory
        } else if metadata.is_symlink() {
            FileKind::Symlink
        } else {
            FileKind::File
        };
        Self {
            kind,
            len: metadata.len(),
            modified: metadata.modified().ok(),
        }
    }
}
