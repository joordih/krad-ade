use krad_domain::{Linger, RunnerState};
use std::borrow::Cow;

use super::super::runtime_view::RunnerReach;

const NOT_CONNECTED: &str = "Not connected";
const CONNECTING: &str = "Connecting";
const INSTALLING: &str = "Installing the runner";
const READY: &str = "Ready";
const UPDATE_PENDING: &str = "Ready; the update starts once no session runs";
const FAILED_PREFIX: &str = "Failed: ";
const REACHING_RUNTIME: &str = "Reaching its runtime";
const RUNTIME_UPDATE_PENDING: &str =
    "Update pending: it keeps only processes until it runs the new version";
const RUNTIME_PREFIX: &str = "Runtime";
const ONE_SESSION: &str = "1 session";
const SESSIONS_SUFFIX: &str = "sessions";
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

pub(in crate::environment_shell) fn reach_label(
    reach: &RunnerReach,
    sessions: usize,
) -> Cow<'static, str> {
    match reach {
        RunnerReach::Connecting => Cow::Borrowed(REACHING_RUNTIME),
        RunnerReach::ProcessWire => Cow::Borrowed(RUNTIME_UPDATE_PENDING),
        RunnerReach::Connected { build } if sessions == 1 => {
            Cow::Owned(format!("{RUNTIME_PREFIX} {build}, {ONE_SESSION}"))
        }
        RunnerReach::Connected { build } => Cow::Owned(format!(
            "{RUNTIME_PREFIX} {build}, {sessions} {SESSIONS_SUFFIX}"
        )),
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

    #[test]
    fn an_older_runner_reads_as_update_pending_and_a_reached_one_counts_its_sessions() {
        assert_eq!(
            reach_label(&RunnerReach::ProcessWire, 0),
            RUNTIME_UPDATE_PENDING
        );
        let connected = RunnerReach::Connected {
            build: "0.9.0".to_owned(),
        };
        assert_eq!(reach_label(&connected, 1), "Runtime 0.9.0, 1 session");
        assert_eq!(reach_label(&connected, 3), "Runtime 0.9.0, 3 sessions");
    }
}
