use krad_domain::InstallId;
use std::path::{Path, PathBuf};

use crate::error::HostError;

pub const APP_DIR: &str = ".krad-ade";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

macro_rules! mux_protocol_version {
    () => {
        2
    };
}

pub(crate) const MUX_PROTOCOL_VERSION: u16 = mux_protocol_version!();
pub const HOST_CLI_BUILD: &str =
    concat!(env!("CARGO_PKG_VERSION"), "+mux", mux_protocol_version!());
pub(crate) const BIN_DIR: &str = "bin";
pub const HOST_CLI_PROGRAM: &str = "krad-ade-agents";
const CLIENTS_DIR: &str = "clients";
const RUNNER_DIR: &str = "runner";
const LANGUAGES_DIR: &str = "languages";
const ENV_DIR: &str = "env";
const RUN_DIR: &str = "run";
const BUS_DIR: &str = "bus";
const ATTACHMENTS_DIR: &str = "attachments";
const LOGIN_ENV_FILE: &str = "login-env";
const CLAUDE_HOOKS_FILE: &str = "claude-hooks.json";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostLayout {
    home: PathBuf,
    install_id: Option<InstallId>,
}

impl HostLayout {
    #[must_use]
    pub fn new(home: &Path, install_id: Option<InstallId>) -> Self {
        Self {
            home: home.to_owned(),
            install_id,
        }
    }

    #[must_use]
    pub fn home(&self) -> &Path {
        &self.home
    }

    #[must_use]
    pub fn app_dir(&self) -> PathBuf {
        self.home.join(APP_DIR)
    }

    #[must_use]
    pub fn bin_dir(&self) -> PathBuf {
        self.app_dir().join(HOST_CLI_BUILD).join(BIN_DIR)
    }

    #[must_use]
    pub fn runner_dir(&self) -> PathBuf {
        self.app_dir().join(RUNNER_DIR)
    }

    #[must_use]
    pub fn languages_dir(&self) -> PathBuf {
        self.app_dir().join(LANGUAGES_DIR)
    }

    /// # Errors
    pub fn client_dir(&self) -> Result<PathBuf, HostError> {
        let id = self.install_id.as_ref().ok_or(HostError::NoInstallId)?;
        Ok(self.app_dir().join(CLIENTS_DIR).join(id.as_str()))
    }

    /// # Errors
    pub fn env_dir(&self) -> Result<PathBuf, HostError> {
        Ok(self.client_dir()?.join(ENV_DIR))
    }

    /// # Errors
    pub fn run_dir(&self) -> Result<PathBuf, HostError> {
        Ok(self.client_dir()?.join(RUN_DIR))
    }

    /// # Errors
    pub fn bus_dir(&self) -> Result<PathBuf, HostError> {
        Ok(self.client_dir()?.join(BUS_DIR))
    }

    /// # Errors
    pub fn attachments_dir(&self) -> Result<PathBuf, HostError> {
        Ok(self.client_dir()?.join(ATTACHMENTS_DIR))
    }

    /// # Errors
    pub fn login_env_file(&self) -> Result<PathBuf, HostError> {
        Ok(self.client_dir()?.join(LOGIN_ENV_FILE))
    }

    /// # Errors
    pub fn claude_hooks_file(&self) -> Result<PathBuf, HostError> {
        Ok(self.client_dir()?.join(CLAUDE_HOOKS_FILE))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn binaries_are_per_version_and_state_is_per_installation() -> TestResult {
        let layout = HostLayout::new(Path::new("/home/me"), Some("0123456789abcdef".parse()?));
        assert_eq!(
            layout.bin_dir(),
            Path::new("/home/me/.krad-ade")
                .join(HOST_CLI_BUILD)
                .join("bin")
        );
        assert_eq!(
            layout.bus_dir()?,
            Path::new("/home/me/.krad-ade/clients/0123456789abcdef/bus")
        );
        assert_eq!(layout.runner_dir(), Path::new("/home/me/.krad-ade/runner"));
        assert_eq!(
            layout.languages_dir(),
            Path::new("/home/me/.krad-ade/languages")
        );
        let anonymous = HostLayout::new(Path::new("/home/me"), None);
        assert!(matches!(anonymous.bus_dir(), Err(HostError::NoInstallId)));
        Ok(())
    }
}
