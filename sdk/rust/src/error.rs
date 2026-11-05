use std::io;

#[derive(Debug, thiserror::Error)]
pub enum SdkError {
    #[error("the app's pipe failed: {0}")]
    Io(#[from] io::Error),
    #[error("unexpected data: {0}")]
    Json(#[from] serde_json::Error),
    #[error("a frame came without its length")]
    Header,
    #[error("a frame of {0} bytes is over the limit")]
    FrameTooLarge(usize),
    #[error("the app stopped")]
    Stopped,
    #[error("the app did not answer in time")]
    Timeout,
    #[error("the app refused the request: {message}")]
    Refused { code: i64, message: String },
    #[error("{0} is answered by the extension, not by the app")]
    WrongDirection(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct HandlerError {
    pub message: String,
}

impl HandlerError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl From<SdkError> for HandlerError {
    fn from(error: SdkError) -> Self {
        Self::new(error.to_string())
    }
}

impl From<serde_json::Error> for HandlerError {
    fn from(error: serde_json::Error) -> Self {
        Self::new(error.to_string())
    }
}

impl From<io::Error> for HandlerError {
    fn from(error: io::Error) -> Self {
        Self::new(error.to_string())
    }
}
