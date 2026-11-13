use krad_chat::{harness_health, Runner};
use krad_domain::{AgentPreset, CloudRunnerId, HarnessHealth, RunnerCli, RunnerLogin, SignIn};
use krad_host::{HostCommand, HostLink};
use krad_protocol::{CliLogin, RunnerLogins};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::time::Duration;

const STATUS_WAIT: Duration = Duration::from_secs(15);
const STATUS_ARGS: [&str; 2] = ["auth", "status"];
const NOT_FOUND: i32 = 127;
const GITHUB_PROGRAM: &str = "gh";
const GITLAB_PROGRAM: &str = "glab";
const AUTH_LOGIN: &[&str] = &["auth", "login"];
const CODEX_LOGIN: &[&str] = &["login", "--device-auth"];

pub(crate) const fn git_program(cli: RunnerCli) -> Option<&'static str> {
    match cli {
        RunnerCli::Github => Some(GITHUB_PROGRAM),
        RunnerCli::Gitlab => Some(GITLAB_PROGRAM),
        RunnerCli::Claude | RunnerCli::Codex | RunnerCli::Opencode => None,
    }
}

pub(crate) const fn login_args(cli: RunnerCli) -> &'static [&'static str] {
    match cli {
        RunnerCli::Codex => CODEX_LOGIN,
        RunnerCli::Claude | RunnerCli::Opencode | RunnerCli::Github | RunnerCli::Gitlab => {
            AUTH_LOGIN
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct Logins(HashMap<CloudRunnerId, BTreeMap<RunnerCli, RunnerLogin>>);

impl Logins {
    pub(crate) fn begin(&mut self, runner: CloudRunnerId, cli: RunnerCli) -> bool {
        let known = self.0.entry(runner).or_default();
        if known.get(&cli) == Some(&RunnerLogin::Checking) {
            return false;
        }
        known.insert(cli, RunnerLogin::Checking);
        true
    }

    pub(crate) fn set(&mut self, runner: CloudRunnerId, cli: RunnerCli, login: RunnerLogin) {
        self.0.entry(runner).or_default().insert(cli, login);
    }

    pub(crate) fn forget(&mut self, runner: CloudRunnerId) {
        self.0.remove(&runner);
    }

    pub(crate) fn shown(&self, runner: CloudRunnerId, claude_token: bool) -> RunnerLogins {
        let logins = self
            .0
            .get(&runner)
            .into_iter()
            .flatten()
            .map(|(cli, login)| CliLogin {
                cli: *cli,
                login: *login,
            })
            .collect();
        RunnerLogins {
            runner,
            logins,
            claude_token,
        }
    }

    pub(crate) fn runners(&self) -> impl Iterator<Item = CloudRunnerId> + '_ {
        self.0.keys().copied()
    }
}

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

#[tracing::instrument(skip_all, fields(?cli))]
pub(crate) fn probe_login(
    link: &HostLink,
    home: &Path,
    cli: RunnerCli,
    program: Option<String>,
) -> RunnerLogin {
    match (cli.harness(), git_program(cli), program) {
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

    #[test]
    fn each_cli_signs_in_with_its_own_arguments() {
        assert_eq!(login_args(RunnerCli::Codex), ["login", "--device-auth"]);
        assert_eq!(git_program(RunnerCli::Github), Some("gh"));
        assert_eq!(git_program(RunnerCli::Claude), None);
    }

    #[test]
    fn a_check_already_running_is_not_started_twice() {
        let mut logins = Logins::default();
        let runner = CloudRunnerId(1);
        assert!(logins.begin(runner, RunnerCli::Codex));
        assert!(!logins.begin(runner, RunnerCli::Codex));
        logins.set(runner, RunnerCli::Codex, RunnerLogin::SignedIn);
        assert!(logins.begin(runner, RunnerCli::Codex));
        let shown = logins.shown(runner, true);
        assert!(shown.claude_token);
        assert_eq!(shown.logins.len(), 1);
    }
}
