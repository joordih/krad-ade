mod graph;

use std::time::SystemTime;

use crate::review::CommitSha;

pub use graph::{commit_graph, GraphRow, Lane, LaneColor, Segment};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommitLabel {
    Branch { name: String, current: bool },
    Remote(String),
    Tag(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryCommit {
    pub sha: CommitSha,
    pub parents: Vec<CommitSha>,
    pub author: String,
    pub time: SystemTime,
    pub labels: Vec<CommitLabel>,
    pub subject: String,
}
