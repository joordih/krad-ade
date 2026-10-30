use thiserror::Error;

#[derive(Debug, Error)]
pub enum ChatError {
    #[error("cannot run the harness: {0}")]
    Io(#[from] std::io::Error),
}
