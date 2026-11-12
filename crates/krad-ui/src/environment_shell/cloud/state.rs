use krad_host::runner::{InstallReport, Linger, ServiceState};
use std::borrow::Cow;

const NOT_CONNECTED: &str = "Not connected";
const CONNECTING: &str = "Connecting";
const INSTALLING: &str = "Installing the runner";
const READY: &str = "Ready";
const UPDATE_PENDING: &str = "Ready; the update starts once no session runs";
const FAILED_PREFIX: &str = "Failed: ";
const LINGER_WARNING: &str = "The host does not keep user services running after you log out, so sessions stop with your last SSH login. Ask its administrator to run `loginctl enable-linger` for your user.";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::environment_shell) enum RunnerState {
    #[default]
    NotConnected,
    Connecting,
    Installing,
    Ready {
        linger: Linger,
    },
    UpdatePending {
        linger: Linger,
    },
    Failed(String),
}

impl From<InstallReport> for RunnerState {
    fn from(report: InstallReport) -> Self {
        match report.service {
            ServiceState::Ready => Self::Ready {
                linger: report.linger,
            },
            ServiceState::UpdatePending => Self::UpdatePending {
                linger: report.linger,
            },
        }
    }
}

impl RunnerState {
    pub(in crate::environment_shell) fn label(&self) -> Cow<'static, str> {
        match self {
            Self::NotConnected => Cow::Borrowed(NOT_CONNECTED),
            Self::Connecting => Cow::Borrowed(CONNECTING),
            Self::Installing => Cow::Borrowed(INSTALLING),
            Self::Ready { .. } => Cow::Borrowed(READY),
            Self::UpdatePending { .. } => Cow::Borrowed(UPDATE_PENDING),
            Self::Failed(reason) => Cow::Owned(format!("{FAILED_PREFIX}{reason}")),
        }
    }

    pub(in crate::environment_shell) const fn is_ready(&self) -> bool {
        matches!(self, Self::Ready { .. } | Self::UpdatePending { .. })
    }

    pub(in crate::environment_shell) const fn is_busy(&self) -> bool {
        matches!(self, Self::Connecting | Self::Installing)
    }

    pub(in crate::environment_shell) const fn linger_warning(&self) -> Option<&'static str> {
        match self {
            Self::Ready {
                linger: Linger::Unavailable,
            }
            | Self::UpdatePending {
                linger: Linger::Unavailable,
            } => Some(LINGER_WARNING),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_install_report_becomes_the_runner_state() {
        let ready = RunnerState::from(InstallReport {
            service: ServiceState::Ready,
            linger: Linger::Enabled,
        });
        assert_eq!(
            ready,
            RunnerState::Ready {
                linger: Linger::Enabled
            }
        );
        assert!(ready.is_ready());
        assert_eq!(ready.linger_warning(), None);
        let pending = RunnerState::from(InstallReport {
            service: ServiceState::UpdatePending,
            linger: Linger::Unavailable,
        });
        assert_eq!(
            pending,
            RunnerState::UpdatePending {
                linger: Linger::Unavailable
            }
        );
        assert!(pending.is_ready());
        assert_eq!(pending.linger_warning(), Some(LINGER_WARNING));
    }

    #[test]
    fn only_a_runner_on_its_way_is_busy_and_failures_tell_why() {
        assert!(RunnerState::Connecting.is_busy());
        assert!(RunnerState::Installing.is_busy());
        assert!(!RunnerState::NotConnected.is_busy());
        assert!(!RunnerState::Failed(String::new()).is_ready());
        assert_eq!(
            RunnerState::Failed("no route".to_owned()).label(),
            "Failed: no route"
        );
        assert_eq!(RunnerState::Installing.label(), INSTALLING);
    }
}
