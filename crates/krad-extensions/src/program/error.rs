use std::io;

use crate::program::rpc::ResponseError;

#[derive(Debug, thiserror::Error)]
pub enum ProgramError {
    #[error("the extension program could not start: {0}")]
    Spawn(io::Error),
    #[error("the extension program stopped")]
    Stopped,
    #[error("the extension program did not answer in time")]
    Timeout,
    #[error("the extension program refused the request: {}", .0.message)]
    Refused(ResponseError),
    #[error("the extension program sent a frame without its length")]
    Header,
    #[error("the extension program sent a frame of {0} bytes, over the limit")]
    FrameTooLarge(usize),
    #[error("the extension program sent unexpected data: {0}")]
    Json(#[from] serde_json::Error),
    #[error("the extension program's pipe failed: {0}")]
    Io(#[from] io::Error),
}
