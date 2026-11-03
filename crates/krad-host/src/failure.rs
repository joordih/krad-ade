use std::fmt;

const SSH_ERROR_STATUS: i32 = 255;
const NAME_RESOLUTION: [&str; 2] = ["Could not resolve hostname", "Name or service not known"];
const CONNECTION_REFUSED: [&str; 1] = ["Connection refused"];
const TIMED_OUT: [&str; 2] = ["Connection timed out", "Operation timed out"];
const HOST_KEY_CHANGED: [&str; 2] = ["REMOTE HOST IDENTIFICATION HAS CHANGED", "Host key for"];
const HOST_KEY_CHANGED_TAIL: &str = "has changed";
const HOST_KEY_UNKNOWN: [&str; 2] = [
    "Host key verification failed",
    "No ED25519 host key is known",
];
const TOO_MANY_FAILURES: [&str; 1] = ["Too many authentication failures"];
const PERMISSION_DENIED: [&str; 1] = ["Permission denied ("];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SshFailure {
    NameResolution,
    ConnectionRefused,
    Timeout,
    HostKeyChanged,
    HostKeyUnknown,
    AuthenticationFailed,
    NeedsInteractiveSignIn,
    Other(String),
}

impl SshFailure {
    #[must_use]
    pub fn classify(stderr: &str) -> Self {
        let has = |patterns: &[&str]| patterns.iter().any(|pattern| stderr.contains(pattern));
        if has(&HOST_KEY_CHANGED[..1])
            || (has(&HOST_KEY_CHANGED[1..]) && stderr.contains(HOST_KEY_CHANGED_TAIL))
        {
            return Self::HostKeyChanged;
        }
        if has(&NAME_RESOLUTION) {
            return Self::NameResolution;
        }
        if has(&CONNECTION_REFUSED) {
            return Self::ConnectionRefused;
        }
        if has(&TIMED_OUT) {
            return Self::Timeout;
        }
        if has(&HOST_KEY_UNKNOWN) {
            return Self::HostKeyUnknown;
        }
        if has(&TOO_MANY_FAILURES) {
            return Self::AuthenticationFailed;
        }
        if has(&PERMISSION_DENIED) {
            return Self::NeedsInteractiveSignIn;
        }
        Self::Other(last_line(stderr).to_owned())
    }

    #[must_use]
    pub const fn is_ssh_status(status: Option<i32>) -> bool {
        matches!(status, Some(SSH_ERROR_STATUS))
    }

    #[must_use]
    pub const fn needs_terminal(&self) -> bool {
        matches!(self, Self::NeedsInteractiveSignIn | Self::HostKeyUnknown)
    }
}

fn last_line(stderr: &str) -> &str {
    stderr
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default()
}

impl fmt::Display for SshFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NameResolution => f.write_str("the host name could not be resolved"),
            Self::ConnectionRefused => f.write_str("the host refused the connection"),
            Self::Timeout => f.write_str("the host did not answer in time"),
            Self::HostKeyChanged => f.write_str(
                "the host's key changed since the last connection; check it before trusting it",
            ),
            Self::HostKeyUnknown => f.write_str("the host's key is not known yet"),
            Self::AuthenticationFailed => f.write_str("the host rejected the credentials"),
            Self::NeedsInteractiveSignIn => {
                f.write_str("signing in needs a password or passphrase")
            }
            Self::Other(message) if message.is_empty() => f.write_str("ssh failed"),
            Self::Other(message) => write!(f, "ssh failed: {message}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openssh_messages_map_to_what_went_wrong() {
        let cases = [
            ("ssh: Could not resolve hostname devbox: nodename nor servname provided", SshFailure::NameResolution),
            ("ssh: connect to host 10.0.0.5 port 22: Connection refused", SshFailure::ConnectionRefused),
            ("ssh: connect to host 10.0.0.5 port 22: Operation timed out", SshFailure::Timeout),
            ("@    WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED!     @", SshFailure::HostKeyChanged),
            ("No ED25519 host key is known for devbox and you have requested strict checking.\nHost key verification failed.", SshFailure::HostKeyUnknown),
            ("me@devbox: Permission denied (publickey,password).", SshFailure::NeedsInteractiveSignIn),
            ("Received disconnect: Too many authentication failures", SshFailure::AuthenticationFailed),
        ];
        for (stderr, failure) in cases {
            assert_eq!(SshFailure::classify(stderr), failure, "{stderr}");
        }
        assert_eq!(
            SshFailure::classify("kex_exchange_identification: read: Connection reset\n"),
            SshFailure::Other("kex_exchange_identification: read: Connection reset".to_owned())
        );
    }
}
