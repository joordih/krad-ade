use krad_domain::TerminalId;
use krad_host::HostPlatform;
use std::time::Instant;

use super::backoff::reconnect_delay;

const CONNECTED_LABEL: &str = "Connected";
const CONNECTING_LABEL: &str = "Connecting…";
const SIGNING_IN_LABEL: &str = "Signing in…";
const DISCONNECTED_LABEL: &str = "Disconnected";
const FAILED_LABEL: &str = "Could not connect";
const NOT_SHOWN_LABEL: &str = "Not connected";
const RECONNECTING_LABEL: &str = "Reconnecting…";
const FIRST_RETRY: u32 = 1;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::environment_shell) enum HostState {
    #[default]
    NotShown,
    Connecting,
    SigningIn {
        terminal: TerminalId,
    },
    SignedIn,
    Connected(HostPlatform),
    Reconnecting {
        attempt: u32,
        due: Instant,
    },
    Disconnected,
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) enum HostTone {
    Ready,
    Pending,
    Off,
    Broken,
}

impl HostState {
    pub(in crate::environment_shell) const fn connects_when_shown(&self) -> bool {
        matches!(self, Self::NotShown)
    }

    pub(in crate::environment_shell) const fn is_connected(&self) -> bool {
        matches!(self, Self::Connected(_))
    }

    pub(in crate::environment_shell) const fn can_reconnect(&self) -> bool {
        matches!(
            self,
            Self::Disconnected | Self::Failed(_) | Self::Reconnecting { .. }
        )
    }

    pub(in crate::environment_shell) const fn is_pending(&self) -> bool {
        matches!(
            self,
            Self::NotShown
                | Self::Connecting
                | Self::SigningIn { .. }
                | Self::SignedIn
                | Self::Reconnecting { .. }
        )
    }

    pub(in crate::environment_shell) fn retry_due(&self, now: Instant) -> Option<u32> {
        match self {
            Self::Reconnecting { attempt, due } if *due <= now => Some(*attempt),
            _ => None,
        }
    }

    pub(in crate::environment_shell) fn retry(attempt: u32, now: Instant) -> Option<Self> {
        let delay = reconnect_delay(attempt)?;
        Some(Self::Reconnecting {
            attempt,
            due: now + delay,
        })
    }

    pub(in crate::environment_shell) const fn sign_in_terminal(&self) -> Option<TerminalId> {
        match self {
            Self::SigningIn { terminal } => Some(*terminal),
            _ => None,
        }
    }

    pub(in crate::environment_shell) fn dropped(&self, now: Instant) -> Option<Self> {
        if !self.is_connected() {
            return None;
        }
        Some(Self::retry(FIRST_RETRY, now).unwrap_or(Self::Disconnected))
    }

    pub(in crate::environment_shell) fn after_sign_in(
        &self,
        terminal: TerminalId,
        code: Option<i32>,
    ) -> Option<Self> {
        if self.sign_in_terminal() != Some(terminal) {
            return None;
        }
        Some(match code {
            Some(0) => Self::SignedIn,
            Some(code) => Self::Failed(format!("ssh exited with code {code}")),
            None => Self::Failed("ssh ended without an exit code".to_owned()),
        })
    }

    pub(in crate::environment_shell) const fn tone(&self) -> HostTone {
        match self {
            Self::Connected(_) => HostTone::Ready,
            Self::Connecting
            | Self::SigningIn { .. }
            | Self::SignedIn
            | Self::Reconnecting { .. } => HostTone::Pending,
            Self::NotShown | Self::Disconnected => HostTone::Off,
            Self::Failed(_) => HostTone::Broken,
        }
    }

    pub(in crate::environment_shell) const fn label(&self) -> &'static str {
        match self {
            Self::NotShown => NOT_SHOWN_LABEL,
            Self::Connecting => CONNECTING_LABEL,
            Self::SigningIn { .. } | Self::SignedIn => SIGNING_IN_LABEL,
            Self::Connected(_) => CONNECTED_LABEL,
            Self::Reconnecting { .. } => RECONNECTING_LABEL,
            Self::Disconnected => DISCONNECTED_LABEL,
            Self::Failed(_) => FAILED_LABEL,
        }
    }

    pub(in crate::environment_shell) fn detail(&self) -> Option<String> {
        match self {
            Self::Connected(platform) => Some(platform.to_string()),
            Self::Failed(message) => Some(message.clone()),
            Self::NotShown
            | Self::Connecting
            | Self::SigningIn { .. }
            | Self::SignedIn
            | Self::Reconnecting { .. }
            | Self::Disconnected => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_host_never_shown_connects_on_its_own() {
        assert!(HostState::NotShown.connects_when_shown());
        assert!(!HostState::Disconnected.connects_when_shown());
        assert!(!HostState::Failed(String::new()).connects_when_shown());
        assert!(!HostState::Connecting.connects_when_shown());
    }

    #[test]
    fn a_drop_schedules_a_retry_only_for_a_connected_host() -> Result<(), &'static str> {
        let now = Instant::now();
        let connected = HostState::Connected(HostPlatform::LinuxX86_64);
        let Some(HostState::Reconnecting { attempt, due }) = connected.dropped(now) else {
            return Err("a connected host retries after a drop");
        };
        assert_eq!(attempt, FIRST_RETRY);
        assert!(due > now);
        assert_eq!(HostState::Connecting.dropped(now), None);
        assert!(HostState::Disconnected.can_reconnect());
        assert!(HostState::Failed("refused".to_owned()).can_reconnect());
        assert!(!connected.can_reconnect());
        Ok(())
    }

    #[test]
    fn the_sign_in_terminal_exit_decides_the_next_step() {
        let terminal = TerminalId(7);
        let signing_in = HostState::SigningIn { terminal };
        assert_eq!(
            signing_in.after_sign_in(terminal, Some(0)),
            Some(HostState::SignedIn)
        );
        assert!(matches!(
            signing_in.after_sign_in(terminal, Some(255)),
            Some(HostState::Failed(_))
        ));
        assert_eq!(signing_in.after_sign_in(TerminalId(8), Some(0)), None);
        assert_eq!(HostState::Connecting.after_sign_in(terminal, Some(0)), None);
    }

    #[test]
    fn tones_follow_the_state() {
        assert_eq!(
            HostState::Connected(HostPlatform::LinuxAarch64).tone(),
            HostTone::Ready
        );
        assert_eq!(
            HostState::SigningIn {
                terminal: TerminalId(1)
            }
            .tone(),
            HostTone::Pending
        );
        assert_eq!(HostState::Disconnected.tone(), HostTone::Off);
        assert_eq!(HostState::Failed(String::new()).tone(), HostTone::Broken);
    }

    #[test]
    fn a_retry_is_due_once_its_delay_passed_and_ends_with_the_schedule() -> Result<(), &'static str>
    {
        let now = Instant::now();
        let state = HostState::retry(2, now).ok_or("the second attempt is scheduled")?;
        assert_eq!(state.retry_due(now), None);
        assert_eq!(
            state.retry_due(now + std::time::Duration::from_mins(5)),
            Some(2)
        );
        assert!(state.is_pending());
        assert!(state.can_reconnect());
        assert_eq!(
            HostState::retry(super::super::backoff::RECONNECT_ATTEMPTS + 1, now),
            None
        );
        Ok(())
    }
}
