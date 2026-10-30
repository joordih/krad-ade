mod claude;

use krad_domain::{ResumeId, RewindId, RewindPoint, RewindScope};
use std::path::PathBuf;
use thiserror::Error;

use crate::command::LiveSession;

pub(crate) use claude::{files_reply, transcript_cut};

pub const DROPS_TURN_REFUSED: &str = "Resume rejected by --resume-drops-turn";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RewindRequest {
    pub point: RewindPoint,
    pub scope: RewindScope,
    pub home: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rewound {
    InPlace,
    Restart(LiveSession),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResumeCut {
    pub from: ResumeId,
    pub at: RewindId,
    pub drops: Option<RewindId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum RewindError {
    #[error("this message cannot be rewound in this chat")]
    UnknownPoint,
    #[error("the chat has no conversation to rewind yet")]
    NoSession,
    #[error("cannot find the folder that keeps the conversation")]
    NoHome,
    #[error("cannot find the conversation file to rewind")]
    NoTranscript,
    #[error("the message is no longer in the conversation")]
    MissingPrompt,
    #[error("the chat is still answering")]
    Busy,
    #[error("this harness cannot restore files")]
    KeepsFiles,
    #[error("the harness did not answer the rewind in time")]
    Timeout,
    #[error("the harness stopped")]
    Stopped,
    #[error("{0}")]
    Refused(String),
}

impl From<std::io::Error> for RewindError {
    fn from(error: std::io::Error) -> Self {
        Self::Refused(error.to_string())
    }
}
