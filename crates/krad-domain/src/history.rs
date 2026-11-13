mod graph;

use serde::{Deserialize, Serialize};
use std::time::SystemTime;

use crate::git::{FileChange, LineCount};

use crate::review::CommitSha;

pub use graph::{commit_graph, GraphRow, Lane, LaneColor, Segment};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitLabel {
    Branch { name: String, current: bool },
    Remote(String),
    Tag(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryCommit {
    pub sha: CommitSha,
    pub parents: Vec<CommitSha>,
    pub author: String,
    pub time: SystemTime,
    pub labels: Vec<CommitLabel>,
    pub subject: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitFile {
    pub change: FileChange,
    pub lines: LineCount,
}
