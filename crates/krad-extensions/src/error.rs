use krad_domain::ToolchainId;
use std::io;

#[derive(Debug, thiserror::Error)]
pub enum ExtensionError {
    #[error("download failed: {0}")]
    Http(#[from] ureq::Error),
    #[error("could not write to disk: {0}")]
    Io(#[from] io::Error),
    #[error("unexpected data: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{file} is not a valid manifest: {reason}")]
    Manifest { file: String, reason: String },
    #[error("{url} does not match its published SHA-256")]
    Digest { url: String },
    #[error("`{0}` failed")]
    Tool(String),
    #[error("{0} was not found after installing it")]
    Missing(String),
    #[error("no build for this platform")]
    NoBuild,
    #[error("language servers are not offered on this platform")]
    Unsupported,
    #[error("template `{0}` could not be filled")]
    Template(String),
    #[error("no toolchain installs `{0}` packages")]
    UnknownKind(String),
    #[error("the package registry has no `{0}`")]
    UnknownPackage(String),
    #[error("{0} is needed but was not found")]
    MissingToolchain(ToolchainId),
}
