mod graph;

use serde::{Deserialize, Serialize};
use std::time::SystemTime;

use crate::git::{FileChange, LineCount};

use crate::review::CommitSha;

pub use graph::{commit_graph, GraphRow, Lane, LaneColor, Segment};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum CommitLabel {
    Branch { name: String, current: bool },
    Remote(String),
    Tag(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct HistoryCommit {
    pub sha: CommitSha,
    pub parents: Vec<CommitSha>,
    pub author: String,
    #[cfg_attr(feature = "ts", ts(as = "crate::ts_shapes::SerdeSystemTime"))]
    pub time: SystemTime,
    pub labels: Vec<CommitLabel>,
    pub subject: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CommitFile {
    pub change: FileChange,
    pub lines: LineCount,
}
