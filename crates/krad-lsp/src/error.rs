use std::io;

#[derive(Debug, thiserror::Error)]
pub enum LspError {
    #[error("could not start the language server: {0}")]
    Spawn(#[source] io::Error),
    #[error("language server pipe failed: {0}")]
    Pipe(#[from] io::Error),
    #[error("malformed language server message: {0}")]
    Json(#[from] serde_json::Error),
    #[error("malformed language server header")]
    Header,
    #[error("the language server has stopped")]
    Stopped,
    #[error("the language server is busy")]
    Busy,
    #[error("the language server answered with error {code}: {message}")]
    Server { code: i64, message: String },
    #[error("{0} is not a file path")]
    NotAPath(String),
}
