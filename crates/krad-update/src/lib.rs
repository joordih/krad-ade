mod error;
mod installation;
mod manifest;
mod release;
mod signature;
mod version;

pub use error::UpdateError;
pub use installation::{AfterInstall, Installation, Reach};
pub use release::{latest_update, AvailableUpdate, StagedUpdate};
pub use version::{AppVersion, VersionError};
