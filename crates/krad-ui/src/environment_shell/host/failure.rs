use krad_host::HostError;
use krad_repo::RepoError;
use std::fmt::Display;

use super::super::EnvironmentShell;

pub(in crate::environment_shell) trait HostFailure: Display {
    fn host_unavailable(&self) -> bool;
}

impl HostFailure for HostError {
    fn host_unavailable(&self) -> bool {
        self.breaks_the_channel()
    }
}

impl HostFailure for RepoError {
    fn host_unavailable(&self) -> bool {
        match self {
            Self::Io(_) => true,
            Self::Host(error) => error.breaks_the_channel(),
            _ => false,
        }
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn toast_failure(&mut self, error: &impl HostFailure) {
        let Some(live) = self.envs.get(&self.active) else {
            return;
        };
        let remote = !live.link.is_local();
        if remote && (live.host.is_pending() || error.host_unavailable()) {
            tracing::debug!(error = %error, "host operation skipped while the host is unavailable");
            return;
        }
        self.push_toast(error.to_string());
    }
}
