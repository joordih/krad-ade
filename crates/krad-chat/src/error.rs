use krad_domain::AgentPreset;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ChatError {
    #[error("{} cannot answer in the chat", .0.display_name())]
    Unsupported(AgentPreset),
    #[error("cannot run the harness: {0}")]
    Io(#[from] std::io::Error),
}
