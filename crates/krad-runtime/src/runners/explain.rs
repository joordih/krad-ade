use krad_domain::SshDestination;
use krad_host::{HostError, SshFailure};

use crate::hosts::explain_host_problem;

pub(crate) fn explain(error: &HostError, destination: &SshDestination) -> String {
    match error {
        HostError::Ssh(SshFailure::NeedsInteractiveSignIn) => format!(
            "{destination} asks for a password or passphrase. Open an SSH environment on {destination}, sign in there once, then reconnect."
        ),
        HostError::Ssh(SshFailure::HostKeyUnknown) => format!(
            "The key of {destination} is not known yet. Open an SSH environment on {destination} to check and accept it, then reconnect."
        ),
        other => explain_host_problem(other, destination),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_asking_for_a_password_sends_the_user_to_sign_in_once(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let destination: SshDestination = "me@gpu".parse()?;
        let text = explain(
            &HostError::Ssh(SshFailure::NeedsInteractiveSignIn),
            &destination,
        );
        assert!(text.contains("Open an SSH environment on me@gpu"));
        let other = explain(&HostError::Ssh(SshFailure::NameResolution), &destination);
        assert!(other.contains("Could not find me@gpu"));
        Ok(())
    }
}
