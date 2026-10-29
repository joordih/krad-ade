use thiserror::Error;

#[derive(Debug, Error)]
pub enum RepoError {
    #[error("file system error: {0}")]
    Io(#[from] std::io::Error),
    #[error("`{command}` failed: {message}")]
    Git { command: String, message: String },
    #[error("`{command}` failed in WSL distro {distro}: {message}")]
    Wsl {
        distro: String,
        command: String,
        message: String,
    },
    #[error("file is not text")]
    Binary,
    #[error("invalid file: {0}")]
    Format(String),
    #[error("`{0}` is not a valid name")]
    InvalidName(String),
    #[error("`{0}` already exists")]
    AlreadyExists(String),
    #[error("a folder cannot move into itself")]
    MoveIntoItself,
    #[error("`{0}` is outside the repository")]
    OutsideRepository(String),
    #[error("could not open the file manager: {0}")]
    Reveal(String),
}
