use gpui_kit::App;
use krad_client::ClientError;
use krad_protocol::ErrorCode;
use std::fmt::Display;

use krad_domain::ToastKind;

use super::super::EnvironmentShell;

pub(in crate::environment_shell) trait HostFailure: Display {
    fn host_unavailable(&self) -> bool;
}

impl HostFailure for ClientError {
    fn host_unavailable(&self) -> bool {
        match self {
            Self::Stopped => true,
            Self::Refused { code, .. } | Self::Failed { code, .. } => {
                *code == ErrorCode::Unavailable
            }
        }
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn toast_failure(
        &mut self,
        error: &impl HostFailure,
        cx: &App,
    ) {
        let Some(live) = self.envs.get(&self.active) else {
            return;
        };
        let remote = !live.data.host.is_local();
        if remote && (self.host_state(&self.active, cx).is_pending() || error.host_unavailable()) {
            tracing::debug!(error = %error, "host operation skipped while the host is unavailable");
            return;
        }
        self.push_toast(ToastKind::Error, error.to_string());
    }
}
