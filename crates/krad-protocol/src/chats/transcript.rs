use krad_domain::{ResumeId, SubagentMeta, TranscriptChunk};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct TranscriptAsk {
    pub resume: ResumeId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<PathBuf>,
    pub from: u64,
    pub first_look: bool,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct TranscriptRead {
    pub resume: ResumeId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<PathBuf>,
    pub chunk: TranscriptChunk,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started: Option<SystemTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    pub start: u64,
}

impl TranscriptRead {
    #[must_use]
    pub const fn skips_history(&self) -> bool {
        self.start > 0
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "from", rename_all = "snake_case")]
pub enum SubagentsFrom {
    Claude {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        root: Option<PathBuf>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        home: Option<PathBuf>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        conversation: Option<PathBuf>,
    },
    Gateway {
        cwd: PathBuf,
    },
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct KnownRecord {
    pub record: PathBuf,
    pub read: u64,
    pub finished: bool,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SubagentsAsk {
    pub resume: ResumeId,
    pub from: SubagentsFrom,
    pub known: Vec<KnownRecord>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SubagentUpdate {
    pub meta: SubagentMeta,
    pub record: PathBuf,
    pub chunk: TranscriptChunk,
}

#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct SubagentsRead {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation: Option<PathBuf>,
    pub updates: Vec<SubagentUpdate>,
}
