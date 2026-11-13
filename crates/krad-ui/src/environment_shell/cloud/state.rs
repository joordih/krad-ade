use krad_domain::{Linger, RunnerState};
use std::borrow::Cow;

const NOT_CONNECTED: &str = "Not connected";
const CONNECTING: &str = "Connecting";
const INSTALLING: &str = "Installing the runner";
const READY: &str = "Ready";
const UPDATE_PENDING: &str = "Ready; the update starts once no session runs";
const FAILED_PREFIX: &str = "Failed: ";
const LINGER_WARNING: &str = "The host does not keep user services running after you log out, so sessions stop with your last SSH login. Ask its administrator to run `loginctl enable-linger` for your user.";

pub(in crate::environment_shell) fn runner_label(state: &RunnerState) -> Cow<'static, str> {
    match state {
        RunnerState::NotConnected => Cow::Borrowed(NOT_CONNECTED),
        RunnerState::Connecting => Cow::Borrowed(CONNECTING),
        RunnerState::Installing => Cow::Borrowed(INSTALLING),
        RunnerState::Ready { .. } => Cow::Borrowed(READY),
        RunnerState::UpdatePending { .. } => Cow::Borrowed(UPDATE_PENDING),
        RunnerState::Failed { reason } => Cow::Owned(format!("{FAILED_PREFIX}{reason}")),
    }
}

pub(in crate::environment_shell) fn linger_warning(state: &RunnerState) -> Option<&'static str> {
    (state.linger() == Some(Linger::Unavailable)).then_some(LINGER_WARNING)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_runner_without_linger_warns_and_failures_tell_why() {
        let ready = RunnerState::Ready {
            linger: Linger::Enabled,
        };
        assert_eq!(linger_warning(&ready), None);
        let pending = RunnerState::UpdatePending {
            linger: Linger::Unavailable,
        };
        assert_eq!(linger_warning(&pending), Some(LINGER_WARNING));
        let failed = RunnerState::Failed {
            reason: "no route".to_owned(),
        };
        assert_eq!(runner_label(&failed), "Failed: no route");
        assert_eq!(runner_label(&RunnerState::Installing), INSTALLING);
    }
}
