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
    #[error("the app sent no heartbeat in time; it may have stopped")]
    NoHeartbeat,
    #[error("could not write the API discovery file: {0}")]
    Host(#[from] krad_host::HostError),
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
