use krad_domain::SshDestination;
use krad_host::{HostError, SshFailure, Waited};

const KNOWN_HOSTS_REMOVE: &str = "ssh-keygen -R";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Recovery {
    RetryAlone,
    WaitForUser,
}

#[must_use]
pub(super) const fn recovery(error: &HostError) -> Recovery {
    match error {
        HostError::Ssh(
            SshFailure::HostKeyChanged
            | SshFailure::HostKeyUnknown
            | SshFailure::AuthenticationFailed
            | SshFailure::NeedsInteractiveSignIn,
        )
        | HostError::UnsupportedPlatform(_)
        | HostError::NoInstallId => Recovery::WaitForUser,
        _ => Recovery::RetryAlone,
    }
}

#[must_use]
pub(super) fn explain(error: &HostError, destination: &SshDestination) -> String {
    match error {
        HostError::Ssh(failure) => explain_ssh(failure, destination),
        HostError::TimedOut(Waited::Connect) => timed_out(destination),
        other => format!("Could not connect to {destination}: {other}"),
    }
}

fn timed_out(destination: &SshDestination) -> String {
    format!("{destination} did not answer in time. Check the network or VPN; the app keeps trying.")
}

fn explain_ssh(failure: &SshFailure, destination: &SshDestination) -> String {
    match failure {
        SshFailure::NameResolution => format!(
            "Could not find {destination}. Check the host name, or the alias in your SSH config."
        ),
        SshFailure::ConnectionRefused => format!(
            "{destination} refused the connection. Check that its SSH server is running and that the port is right."
        ),
        SshFailure::Timeout => timed_out(destination),
        SshFailure::HostKeyChanged => format!(
            "The key of {destination} changed since the last connection. That happens when the server is reinstalled, but it can also mean someone is intercepting the connection. Confirm the new key with the server's owner, then remove the old one with `{KNOWN_HOSTS_REMOVE} {}` and reconnect.",
            known_hosts_name(destination)
        ),
        SshFailure::HostKeyUnknown => format!(
            "The key of {destination} is not known yet. Reconnect to check and accept it."
        ),
        SshFailure::AuthenticationFailed => format!(
            "{destination} rejected the sign-in. Check the user name and the key or password, then reconnect."
        ),
        SshFailure::NeedsInteractiveSignIn => format!(
            "{destination} asks for a password or passphrase. Reconnect to sign in."
        ),
        SshFailure::Other(message) if message.is_empty() => {
            format!("Could not connect to {destination}.")
        }
        SshFailure::Other(message) => format!("Could not connect to {destination}: {message}"),
    }
}

fn known_hosts_name(destination: &SshDestination) -> String {
    match destination.port {
        Some(port) => format!("'[{}]:{port}'", destination.host),
        None => destination.host.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn credentials_and_host_keys_wait_for_the_user() {
        for failure in [
            SshFailure::HostKeyChanged,
            SshFailure::HostKeyUnknown,
            SshFailure::AuthenticationFailed,
            SshFailure::NeedsInteractiveSignIn,
        ] {
            assert_eq!(recovery(&HostError::Ssh(failure)), Recovery::WaitForUser);
        }
        for failure in [
            SshFailure::NameResolution,
            SshFailure::ConnectionRefused,
            SshFailure::Timeout,
        ] {
            assert_eq!(recovery(&HostError::Ssh(failure)), Recovery::RetryAlone);
        }
        assert_eq!(recovery(&HostError::NotConnected), Recovery::RetryAlone);
    }

    #[test]
    fn a_changed_key_names_the_risk_and_the_fix() -> TestResult {
        let destination: SshDestination = "me@devbox:2200".parse()?;
        let text = explain(&HostError::Ssh(SshFailure::HostKeyChanged), &destination);
        assert!(text.contains("intercepting"));
        assert!(text.contains("ssh-keygen -R '[devbox]:2200'"));
        Ok(())
    }

    #[test]
    fn password_hosts_are_told_to_sign_in_not_to_use_an_agent() -> TestResult {
        let destination: SshDestination = "devbox".parse()?;
        let text = explain(
            &HostError::Ssh(SshFailure::NeedsInteractiveSignIn),
            &destination,
        );
        assert!(text.contains("Reconnect to sign in"));
        assert!(!text.contains("agent"));
        Ok(())
    }
}
