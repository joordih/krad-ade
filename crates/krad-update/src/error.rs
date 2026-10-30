use std::io;

use crate::version::VersionError;

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("couldn't reach the release on GitHub: {0}")]
    Http(#[from] ureq::Error),
    #[error("the release manifest is not valid: {0}")]
    Manifest(#[from] serde_json::Error),
    #[error("the release signature doesn't match: {0}")]
    Signature(#[from] minisign_verify::Error),
    #[error(transparent)]
    Version(#[from] VersionError),
    #[error("couldn't write the update to disk: {0}")]
    Io(#[from] io::Error),
    #[error("the release has no build for this platform")]
    NoBuild,
    #[error("the download has {received} bytes instead of {expected}")]
    Size { expected: u64, received: u64 },
    #[error("`{0}` couldn't unpack or install the update")]
    Tool(&'static str),
    #[error("the update archive doesn't contain {0}")]
    MissingPayload(String),
}
