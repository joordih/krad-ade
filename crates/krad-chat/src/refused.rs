use std::fmt::Display;
use thiserror::Error;

const MISSING_TEXT: &str = "No conversation found with session ID";

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ResumeRefused {
    #[error("{0}")]
    MissingConversation(String),
    #[error("{0}")]
    TurnNotDropped(String),
}

impl ResumeRefused {
    pub(crate) fn missing(session: impl Display) -> Self {
        Self::MissingConversation(format!("{MISSING_TEXT} {session}"))
    }
}
