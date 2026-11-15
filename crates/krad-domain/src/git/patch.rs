mod parse;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::LineCount;
use crate::{ChangeKind, GitObjectId, HunkFingerprint};

pub use parse::parse_patch;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum PatchStatus {
    Added,
    Deleted,
    Modified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum HunkLineKind {
    Context,
    Added,
    Removed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct HunkLine {
    pub kind: HunkLineKind,
    pub text: String,
    pub newline: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Hunk {
    pub old_start: usize,
    pub new_start: usize,
    pub lines: Vec<HunkLine>,
    pub fingerprint: HunkFingerprint,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "body", content = "of", rename_all = "snake_case")]
pub enum PatchBody {
    Text(Vec<Hunk>),
    Binary,
    Submodule,
    Renamed { from: PathBuf },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FilePatch {
    pub path: PathBuf,
    pub status: PatchStatus,
    pub old_id: Option<GitObjectId>,
    pub new_id: Option<GitObjectId>,
    pub body: PatchBody,
    pub fingerprint: HunkFingerprint,
}

impl From<PatchStatus> for ChangeKind {
    fn from(status: PatchStatus) -> Self {
        match status {
            PatchStatus::Added => Self::Added,
            PatchStatus::Deleted => Self::Deleted,
            PatchStatus::Modified => Self::Modified,
        }
    }
}

impl FilePatch {
    #[must_use]
    pub fn hunks(&self) -> &[Hunk] {
        match &self.body {
            PatchBody::Text(hunks) => hunks,
            PatchBody::Binary | PatchBody::Submodule | PatchBody::Renamed { .. } => &[],
        }
    }

    #[must_use]
    pub fn fingerprints(&self) -> Vec<HunkFingerprint> {
        match &self.body {
            PatchBody::Text(hunks) if !hunks.is_empty() => {
                hunks.iter().map(|hunk| hunk.fingerprint).collect()
            }
            PatchBody::Text(_)
            | PatchBody::Binary
            | PatchBody::Submodule
            | PatchBody::Renamed { .. } => {
                vec![self.fingerprint]
            }
        }
    }

    #[must_use]
    pub fn line_count(&self) -> LineCount {
        self.hunks().iter().flat_map(|hunk| &hunk.lines).fold(
            LineCount::default(),
            |count, line| match line.kind {
                HunkLineKind::Context => count,
                HunkLineKind::Added => {
                    count
                        + LineCount {
                            added: 1,
                            removed: 0,
                        }
                }
                HunkLineKind::Removed => {
                    count
                        + LineCount {
                            added: 0,
                            removed: 1,
                        }
                }
            },
        )
    }
}
