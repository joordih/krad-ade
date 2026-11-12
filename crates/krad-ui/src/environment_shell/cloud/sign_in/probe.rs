use krad_chat::{harness_health, Runner};
use krad_domain::{AgentPreset, HarnessHealth, SignIn};
use krad_host::{HostCommand, HostLink};
use std::path::Path;
use std::time::Duration;

use super::{RunnerCli, RunnerLogin};

const STATUS_WAIT: Duration = Duration::from_secs(15);
const STATUS_ARGS: [&str; 2] = ["auth", "status"];
const NOT_FOUND: i32 = 127;

fn health_login(health: &HarnessHealth) -> RunnerLogin {
    match health {
        HarnessHealth::Missing
        | HarnessHealth::OffPath { .. }
        | HarnessHealth::Found { version: None, .. } => RunnerLogin::Missing,
        HarnessHealth::Found {
            sign_in: SignIn::SignedIn,
            ..
        } => RunnerLogin::SignedIn,
        HarnessHealth::Found {
            sign_in: SignIn::SignedOut,
            ..
        } => RunnerLogin::SignedOut,
        HarnessHealth::Found {
            sign_in: SignIn::Unknown,
            ..
        } => RunnerLogin::Unknown,
    }
}

const fn exit_login(code: Option<i32>) -> RunnerLogin {
    match code {
        Some(0) => RunnerLogin::SignedIn,
        Some(NOT_FOUND) => RunnerLogin::Missing,
        Some(_) => RunnerLogin::SignedOut,
        None => RunnerLogin::Unknown,
    }
}

fn harness_login(
    link: &HostLink,
    home: &Path,
    harness: AgentPreset,
    program: String,
) -> RunnerLogin {
    let runner = Runner::Ssh {
        link: link.clone(),
        program,
    };
    health_login(&harness_health(&runner, home, harness))
}

fn git_login(link: &HostLink, program: &str) -> RunnerLogin {
    let mut command = HostCommand::new(program);
    command.args(STATUS_ARGS).in_login_shell();
    match link.output_within(&command, STATUS_WAIT) {
        Ok(output) => exit_login(output.status.code()),
        Err(error) => {
            tracing::debug!(%error, "a git sign-in check did not finish on the runner");
            RunnerLogin::Unknown
        }
    }
}

#[tracing::instrument(skip_all, fields(cli = cli.label()))]
pub(super) fn probe_login(
    link: &HostLink,
    home: &Path,
    cli: RunnerCli,
    program: Option<String>,
) -> RunnerLogin {
    match (cli.harness(), cli.git_program(), program) {
        (Some(harness), _, Some(program)) => harness_login(link, home, harness, program),
        (_, Some(git), _) => git_login(link, git),
        _ => RunnerLogin::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::HarnessVersion;

    #[test]
    fn a_harness_check_tells_whether_the_runner_is_signed_in() {
        let found = |version, sign_in| HarnessHealth::Found { version, sign_in };
        let version = Some(HarnessVersion::new(2, 1, 0));
        assert_eq!(
            health_login(&found(version, SignIn::SignedOut)),
            RunnerLogin::SignedOut
        );
        assert_eq!(
            health_login(&found(version, SignIn::SignedIn)),
            RunnerLogin::SignedIn
        );
        assert_eq!(
            health_login(&found(None, SignIn::Unknown)),
            RunnerLogin::Missing
        );
        assert_eq!(health_login(&HarnessHealth::Missing), RunnerLogin::Missing);
    }

    #[test]
    fn a_git_cli_answers_with_its_exit_code() {
        assert_eq!(exit_login(Some(0)), RunnerLogin::SignedIn);
        assert_eq!(exit_login(Some(1)), RunnerLogin::SignedOut);
        assert_eq!(exit_login(Some(NOT_FOUND)), RunnerLogin::Missing);
        assert_eq!(exit_login(None), RunnerLogin::Unknown);
    }
}
