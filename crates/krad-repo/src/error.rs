use thiserror::Error;

impl From<krad_host::HostError> for RepoError {
    fn from(error: krad_host::HostError) -> Self {
        match error {
            krad_host::HostError::Io(error) => Self::Io(error),
            other => Self::Host(other),
        }
    }
}

impl RepoError {
    #[must_use]
    pub fn is_not_found(&self) -> bool {
        match self {
            Self::Io(error) => error.kind() == std::io::ErrorKind::NotFound,
            Self::Host(error) => error.is_not_found(),
            _ => false,
        }
    }
}

#[derive(Debug, Error)]
pub enum RepoError {
    #[error("file system error: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Host(krad_host::HostError),
    #[error("`{command}` failed: {message}")]
    Git { command: String, message: String },
    #[error("`{command}` failed in WSL distro {distro}: {message}")]
    Wsl {
        distro: String,
        command: String,
        message: String,
    },
    #[error("`{0}` changed since its diff was read")]
    Stale(String),
    #[error("`{0}` cannot be reverted from here")]
    Unsupported(String),
    #[error("`{0}` printed something unexpected")]
    UnexpectedOutput(String),
    #[error("file is not text")]
    Binary,
    #[error("invalid file: {0}")]
    Format(String),
    #[error("`{0}` is not a valid name")]
    InvalidName(String),
    #[error("`{0}` has uncommitted changes; commit or stash them first")]
    Uncommitted(String),
    #[error("`{0}` already exists")]
    AlreadyExists(String),
    #[error("a folder cannot move into itself")]
    MoveIntoItself,
    #[error("`{0}` is outside the repository")]
    OutsideRepository(String),
    #[error("could not open the file manager: {0}")]
    Reveal(String),
    #[error("the file manager only opens folders on this computer")]
    NotOnThisComputer,
    #[error("the worktree has uncommitted changes")]
    UncommittedChanges,
    #[error("the worktree has {0} commit(s) not merged into the repository's checked-out branch")]
    UnmergedCommits(usize),
}
