use crate::failure::SshFailure;
use crate::mux::MuxError;
use crate::platform::UnsupportedPlatform;
use crate::sftp::SftpError;
use std::io;
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Waited {
    Connect,
    Command,
    Request,
    Forward,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForwardFailure {
    NoPortAllocated,
    NeverListened,
    Rejected,
}

#[derive(Debug, Error)]
pub enum HostError {
    #[error("file system error: {0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    Sftp(#[from] SftpError),
    #[error("the host is not connected")]
    NotConnected,
    #[error("{0}")]
    Ssh(SshFailure),
    #[error(transparent)]
    UnsupportedPlatform(#[from] UnsupportedPlatform),
    #[error("`{program}` failed on the host: {stderr}")]
    Remote {
        program: String,
        status: Option<i32>,
        stderr: String,
    },
    #[error("the host took too long ({0:?})")]
    TimedOut(Waited),
    #[error("the port forward failed ({0:?})")]
    Forward(ForwardFailure),
    #[error("this installation has no id, so it keeps no files on the host")]
    NoInstallId,
    #[error(transparent)]
    Mux(#[from] MuxError),
    #[error("`{0}` is not a valid environment variable name")]
    InvalidEnvName(String),
    #[error("refusing to remove {0}")]
    UnsafeRemoval(String),
    #[error("the extension that reaches this machine has not opened a channel to it")]
    NoMachineChannel,
    #[error("the channel to the machine closed: {0}")]
    MachineChannel(String),
}

impl From<HostError> for io::Error {
    fn from(error: HostError) -> Self {
        match error {
            HostError::Io(error) => error,
            HostError::TimedOut(_) => Self::new(io::ErrorKind::TimedOut, error),
            other if other.is_not_found() => Self::new(io::ErrorKind::NotFound, other),
            other => Self::other(other),
        }
    }
}

impl HostError {
    #[must_use]
    pub fn is_not_found(&self) -> bool {
        match self {
            Self::Io(error) => error.kind() == io::ErrorKind::NotFound,
            Self::Sftp(error) => error.is_not_found(),
            _ => false,
        }
    }

    #[must_use]
    pub const fn needs_sign_in(&self) -> bool {
        matches!(self, Self::Ssh(failure) if failure.needs_terminal())
    }

    #[must_use]
    pub const fn breaks_the_channel(&self) -> bool {
        matches!(
            self,
            Self::Io(_)
                | Self::NotConnected
                | Self::TimedOut(_)
                | Self::Sftp(SftpError::Io(_) | SftpError::TimedOut)
                | Self::Mux(MuxError::Closed | MuxError::Io(_))
        )
    }
}
