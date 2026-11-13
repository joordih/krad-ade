use krad_protocol::HostState;

const CONNECTED_LABEL: &str = "Connected";
const CONNECTING_LABEL: &str = "Connecting…";
const SIGNING_IN_LABEL: &str = "Signing in…";
const DISCONNECTED_LABEL: &str = "Disconnected";
const FAILED_LABEL: &str = "Could not connect";
const NOT_SHOWN_LABEL: &str = "Not connected";
const RECONNECTING_LABEL: &str = "Reconnecting…";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HostTone {
    Ready,
    Pending,
    Off,
    Broken,
}

pub(super) const fn tone(state: &HostState) -> HostTone {
    match state {
        HostState::Connected { .. } => HostTone::Ready,
        HostState::Connecting
        | HostState::SigningIn { .. }
        | HostState::SignedIn
        | HostState::Reconnecting { .. } => HostTone::Pending,
        HostState::NotShown | HostState::Disconnected => HostTone::Off,
        HostState::Failed { .. } => HostTone::Broken,
    }
}

pub(super) const fn label(state: &HostState) -> &'static str {
    match state {
        HostState::NotShown => NOT_SHOWN_LABEL,
        HostState::Connecting => CONNECTING_LABEL,
        HostState::SigningIn { .. } | HostState::SignedIn => SIGNING_IN_LABEL,
        HostState::Connected { .. } => CONNECTED_LABEL,
        HostState::Reconnecting { .. } => RECONNECTING_LABEL,
        HostState::Disconnected => DISCONNECTED_LABEL,
        HostState::Failed { .. } => FAILED_LABEL,
    }
}

pub(super) fn detail(state: &HostState) -> Option<String> {
    match state {
        HostState::Connected { platform } => Some(platform.to_string()),
        HostState::Failed { reason } => Some(reason.clone()),
        HostState::NotShown
        | HostState::Connecting
        | HostState::SigningIn { .. }
        | HostState::SignedIn
        | HostState::Reconnecting { .. }
        | HostState::Disconnected => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{HostPlatform, TerminalId};

    #[test]
    fn tones_follow_the_state() {
        assert_eq!(
            tone(&HostState::Connected {
                platform: HostPlatform::LinuxAarch64
            }),
            HostTone::Ready
        );
        assert_eq!(
            tone(&HostState::SigningIn {
                terminal: TerminalId(1)
            }),
            HostTone::Pending
        );
        assert_eq!(tone(&HostState::Disconnected), HostTone::Off);
        assert_eq!(
            tone(&HostState::Failed {
                reason: String::new()
            }),
            HostTone::Broken
        );
    }

    #[test]
    fn only_a_connected_or_failed_host_has_a_detail() {
        assert_eq!(
            detail(&HostState::Connected {
                platform: HostPlatform::LinuxX86_64
            }),
            Some("linux-x86_64".to_owned())
        );
        assert_eq!(detail(&HostState::Reconnecting { attempt: 2 }), None);
        assert_eq!(label(&HostState::SignedIn), SIGNING_IN_LABEL);
    }
}
