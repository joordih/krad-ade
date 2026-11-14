use krad_domain::{CloudRunnerId, EnvironmentName, Pairing};
use std::fmt;
use std::path::PathBuf;

pub const OWN_RUNNER: CloudRunnerId = CloudRunnerId(0);

#[derive(Clone)]
pub struct OwnRunner {
    pub cli: PathBuf,
    pub pairing: Pairing,
    pub environment: EnvironmentName,
}

impl fmt::Debug for OwnRunner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OwnRunner")
            .field("cli", &self.cli)
            .field("environment", &self.environment)
            .finish_non_exhaustive()
    }
}
