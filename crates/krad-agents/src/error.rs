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

pub use krad_domain::agents_api::HandleError;
