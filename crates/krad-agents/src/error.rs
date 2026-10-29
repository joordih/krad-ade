use std::num::ParseIntError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentsError {
    #[error("agent bus I/O failure: {0}")]
    Io(#[from] std::io::Error),
    #[error("malformed agent bus message: {0}")]
    Json(#[from] serde_json::Error),
    #[error("the agent bus closed the connection without answering")]
    ConnectionClosed,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum HandleError {
    #[error("terminal handle must look like `<environment>:<terminal id>`")]
    MissingSeparator,
    #[error("terminal handle has an empty environment")]
    EmptyEnvironment,
    #[error("terminal handle has an invalid terminal or group id: {0}")]
    InvalidTerminal(#[from] ParseIntError),
}
