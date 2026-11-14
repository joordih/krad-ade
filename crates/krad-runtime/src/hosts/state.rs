use krad_domain::TerminalId;
use krad_host::HostError;
use krad_protocol::HostState;
use std::time::Instant;

use super::backoff::reconnect_delay;
use super::problem::{recovery, Recovery};

const FIRST_RETRY: u32 = 1;
const SSH_ENDED_WITHOUT_CODE: &str = "ssh ended without an exit code";
pub(crate) const SIGN_IN_NOT_KEPT: &str =
    "Signed in, but the connection did not stay open. Reconnect to try again.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Attempt {
    First,
    AfterSignIn,
    Retry(u32),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Tracked {
    pub(crate) state: HostState,
    due: Option<Instant>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AfterFailure {
    SignIn,
    Next(Tracked),
}

impl From<HostState> for Tracked {
    fn from(state: HostState) -> Self {
        Self { state, due: None }
    }
}

impl Tracked {
    pub(crate) fn retry(attempt: u32, now: Instant) -> Option<Self> {
        let delay = reconnect_delay(attempt)?;
        Some(Self {
            state: HostState::Reconnecting { attempt },
            due: Some(now + delay),
        })
    }

    pub(crate) fn retry_due(&self, now: Instant) -> Option<u32> {
        match (&self.state, self.due) {
            (HostState::Reconnecting { attempt }, Some(due)) if due <= now => Some(*attempt),
            _ => None,
        }
    }

    pub(crate) fn dropped(&self, now: Instant) -> Option<Self> {
        if !self.state.is_connected() {
            return None;
        }
        Some(Self::retry(FIRST_RETRY, now).unwrap_or_else(|| HostState::Disconnected.into()))
    }

    pub(crate) fn after_sign_in(&self, terminal: TerminalId, code: Option<i32>) -> Option<Self> {
        if self.state.sign_in_terminal() != Some(terminal) {
            return None;
        }
        let state = match sign_in_ended(code) {
            Ok(()) => HostState::SignedIn,
            Err(reason) => HostState::Failed { reason },
        };
        Some(state.into())
    }
}

pub(crate) fn sign_in_ended(code: Option<i32>) -> Result<(), String> {
    match code {
        Some(0) => Ok(()),
        Some(code) => Err(format!("ssh exited with code {code}")),
        None => Err(SSH_ENDED_WITHOUT_CODE.to_owned()),
    }
}

pub(crate) fn after_failure(
    attempt: Attempt,
    error: &HostError,
    now: Instant,
    explain: impl FnOnce() -> String,
) -> AfterFailure {
    let failed = |reason: String| AfterFailure::Next(HostState::Failed { reason }.into());
    match attempt {
        Attempt::First if error.needs_sign_in() => AfterFailure::SignIn,
        Attempt::AfterSignIn if error.needs_sign_in() => failed(SIGN_IN_NOT_KEPT.to_owned()),
        Attempt::Retry(done) if recovery(error) == Recovery::RetryAlone => {
            Tracked::retry(done + 1, now).map_or_else(|| failed(explain()), AfterFailure::Next)
        }
        Attempt::First | Attempt::AfterSignIn | Attempt::Retry(_) => failed(explain()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::HostPlatform;
    use krad_host::SshFailure;
    use std::time::Duration;

    const LATER: Duration = Duration::from_mins(5);

    fn explained() -> String {
        "explained".to_owned()
    }

    #[test]
    fn a_drop_schedules_a_retry_only_for_a_connected_host() -> Result<(), &'static str> {
        let now = Instant::now();
        let connected = Tracked::from(HostState::Connected {
            platform: HostPlatform::LinuxX86_64,
            home: None,
            login_env: krad_protocol::LoginEnv::Read,
        });
        let dropped = connected.dropped(now).ok_or("a connected host retries")?;
        assert_eq!(
            dropped.state,
            HostState::Reconnecting {
                attempt: FIRST_RETRY
            }
        );
        assert_eq!(dropped.retry_due(now), None);
        assert_eq!(dropped.retry_due(now + LATER), Some(FIRST_RETRY));
        assert_eq!(Tracked::from(HostState::Connecting).dropped(now), None);
        Ok(())
    }

    #[test]
    fn the_sign_in_terminal_exit_decides_the_next_step() {
        let terminal = TerminalId(7);
        let signing_in = Tracked::from(HostState::SigningIn { terminal });
        assert_eq!(
            signing_in.after_sign_in(terminal, Some(0)),
            Some(HostState::SignedIn.into())
        );
        assert!(matches!(
            signing_in.after_sign_in(terminal, Some(255)),
            Some(Tracked {
                state: HostState::Failed { .. },
                ..
            })
        ));
        assert_eq!(signing_in.after_sign_in(TerminalId(8), Some(0)), None);
        assert_eq!(
            Tracked::from(HostState::Connecting).after_sign_in(terminal, Some(0)),
            None
        );
    }

    #[test]
    fn a_retry_ends_with_the_schedule() {
        let now = Instant::now();
        assert!(Tracked::retry(2, now).is_some());
        assert_eq!(
            Tracked::retry(super::super::backoff::RECONNECT_ATTEMPTS + 1, now),
            None
        );
    }

    #[test]
    fn only_a_first_attempt_signs_in_and_only_a_retry_retries() -> Result<(), &'static str> {
        let now = Instant::now();
        let asks = HostError::Ssh(SshFailure::NeedsInteractiveSignIn);
        let refused = HostError::Ssh(SshFailure::ConnectionRefused);
        assert_eq!(
            after_failure(Attempt::First, &asks, now, explained),
            AfterFailure::SignIn
        );
        assert_eq!(
            after_failure(Attempt::AfterSignIn, &asks, now, explained),
            AfterFailure::Next(
                HostState::Failed {
                    reason: SIGN_IN_NOT_KEPT.to_owned()
                }
                .into()
            )
        );
        assert_eq!(
            after_failure(Attempt::First, &refused, now, explained),
            AfterFailure::Next(
                HostState::Failed {
                    reason: explained()
                }
                .into()
            )
        );
        let AfterFailure::Next(retried) =
            after_failure(Attempt::Retry(2), &refused, now, explained)
        else {
            return Err("a refused retry is not a sign-in");
        };
        assert_eq!(retried.state, HostState::Reconnecting { attempt: 3 });
        assert_eq!(
            after_failure(Attempt::Retry(2), &asks, now, explained),
            AfterFailure::Next(
                HostState::Failed {
                    reason: explained()
                }
                .into()
            )
        );
        Ok(())
    }
}
