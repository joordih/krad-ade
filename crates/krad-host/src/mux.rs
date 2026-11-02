mod askpass;
mod bootstrap;
mod bridge;
mod broker;
mod carrier;
mod channel;
mod forward;
mod prompt;
mod serve;
mod session;
mod shim;
#[cfg(all(test, unix))]
mod tests;
mod wire;

pub use askpass::{is_askpass_invocation, ASKPASS_PROGRAM};
pub use bootstrap::BootstrapFailure;
pub(crate) use carrier::{MuxCarrier, MuxTools};
pub(crate) use channel::Channel;
pub(crate) use forward::MuxForward;
pub use prompt::{PromptKind, PromptSender, Secret, SignInPrompt};
pub use serve::serve;
pub use shim::{run_askpass, run_exec};
pub(crate) use wire::OpenRequest;
pub(crate) use wire::Stream;

use std::io;
use thiserror::Error;

use crate::platform::HostPlatform;

pub const ENV_ENDPOINT: &str = "KRAD_ADE_MUX_ENDPOINT";
pub const ENV_TOKEN: &str = "KRAD_ADE_MUX_TOKEN";
pub const SERVE_VERB: &str = "serve";
pub const EXEC_VERB: &str = "mux-exec";
const NOT_ACCEPTING: &str = "the other end accepts no new channels";
const SESSION_ENDED: &str = "the session ended";

#[derive(Debug, Error)]
pub enum MuxError {
    #[error("the multiplexed session failed: {0}")]
    Io(#[from] io::Error),
    #[error("the multiplexed session ended")]
    Closed,
    #[error("the other end speaks multiplexer protocol {0}, not this app's")]
    Incompatible(u16),
    #[error("the other end sent an unexpected message")]
    Protocol,
    #[error("the other end did not answer in time")]
    TimedOut,
    #[error("the host refused: {0}")]
    Refused(String),
    #[error("{0} hosts are not supported")]
    Unsupported(String),
    #[error("this app has no host CLI for {0}")]
    NoHostCli(HostPlatform),
    #[error("this app has no local CLI to carry commands")]
    NoLocalCli,
    #[error(transparent)]
    Bootstrap(BootstrapFailure),
}

impl From<MuxError> for io::Error {
    fn from(error: MuxError) -> Self {
        match error {
            MuxError::Io(error) => error,
            MuxError::TimedOut => Self::new(io::ErrorKind::TimedOut, error),
            MuxError::Closed => Self::new(io::ErrorKind::BrokenPipe, error),
            other => Self::other(other),
        }
    }
}
