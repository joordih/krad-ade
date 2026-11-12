mod callback;
mod probe;
mod terminal;

pub(in crate::environment_shell) use callback::LoopbackCallback;
pub(in crate::environment_shell) use terminal::SignInWatch;

use gpui_kit::{AppContext as _, Context};
use krad_domain::{AgentPreset, CloudRunnerId};
use krad_host::HostLink;
use std::path::Path;

use super::super::EnvironmentShell;
use probe::probe_login;

const GITHUB_PROGRAM: &str = "gh";
const GITLAB_PROGRAM: &str = "glab";
const AUTH_LOGIN: &[&str] = &["auth", "login"];
const CODEX_LOGIN: &[&str] = &["login", "--device-auth"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::environment_shell) enum RunnerCli {
    Claude,
    Codex,
    Opencode,
    Github,
    Gitlab,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) enum RunnerLogin {
    Checking,
    SignedIn,
    SignedOut,
    Missing,
    Unknown,
}

impl RunnerCli {
    pub(in crate::environment_shell) const ALL: [Self; 5] = [
        Self::Claude,
        Self::Codex,
        Self::Opencode,
        Self::Github,
        Self::Gitlab,
    ];

    pub(in crate::environment_shell) const fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::Opencode => "opencode",
            Self::Github => "GitHub CLI",
            Self::Gitlab => "GitLab CLI",
        }
    }

    pub(in crate::environment_shell) const fn harness(self) -> Option<AgentPreset> {
        match self {
            Self::Claude => Some(AgentPreset::ClaudeCode),
            Self::Codex => Some(AgentPreset::Codex),
            Self::Opencode => Some(AgentPreset::Opencode),
            Self::Github | Self::Gitlab => None,
        }
    }

    pub(in crate::environment_shell) fn of_harness(harness: AgentPreset) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|cli| cli.harness() == Some(harness))
    }

    const fn git_program(self) -> Option<&'static str> {
        match self {
            Self::Github => Some(GITHUB_PROGRAM),
            Self::Gitlab => Some(GITLAB_PROGRAM),
            Self::Claude | Self::Codex | Self::Opencode => None,
        }
    }

    const fn login_args(self) -> &'static [&'static str] {
        match self {
            Self::Codex => CODEX_LOGIN,
            Self::Claude | Self::Opencode | Self::Github | Self::Gitlab => AUTH_LOGIN,
        }
    }
}

impl RunnerLogin {
    pub(in crate::environment_shell) const fn needs_sign_in(self) -> bool {
        matches!(self, Self::SignedOut)
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn runner_login(
        &self,
        runner: CloudRunnerId,
        cli: RunnerCli,
    ) -> Option<RunnerLogin> {
        if cli == RunnerCli::Claude && self.runner_claude_token(runner).is_some() {
            return Some(RunnerLogin::SignedIn);
        }
        self.cloud.logins.get(&(runner, cli)).copied()
    }

    pub(in crate::environment_shell) fn runner_logins(
        &self,
        runner: CloudRunnerId,
    ) -> Vec<(RunnerCli, RunnerLogin)> {
        RunnerCli::ALL
            .into_iter()
            .filter_map(|cli| Some((cli, self.runner_login(runner, cli)?)))
            .collect()
    }

    pub(in crate::environment_shell) fn check_runner_logins(
        &mut self,
        runner: CloudRunnerId,
        cx: &mut Context<Self>,
    ) {
        let Some(link) = self.runner_link(runner) else {
            self.ensure_runner_ready(runner, cx);
            return;
        };
        let HostLink::Remote(ssh) = &link else {
            return;
        };
        let Some(home) = ssh.home().map(Path::to_path_buf) else {
            return;
        };
        for cli in RunnerCli::ALL {
            if self.cloud.logins.get(&(runner, cli)) == Some(&RunnerLogin::Checking) {
                continue;
            }
            self.cloud
                .logins
                .insert((runner, cli), RunnerLogin::Checking);
            let program = cli
                .harness()
                .and_then(|harness| self.launch_program(harness).ok());
            let link = link.clone();
            let home = home.clone();
            let probed =
                cx.background_spawn(async move { probe_login(&link, &home, cli, program) });
            cx.spawn(async move |this, cx| {
                let login = probed.await;
                this.update(cx, |shell, cx| {
                    shell.cloud.logins.insert((runner, cli), login);
                    cx.notify();
                })
                .unwrap_or_default();
            })
            .detach();
        }
        cx.notify();
    }

    pub(in crate::environment_shell) fn forget_runner_logins(&mut self, runner: CloudRunnerId) {
        self.cloud.logins.retain(|(known, _), _| *known != runner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_harness_signs_in_with_its_own_cli() {
        assert_eq!(
            RunnerCli::of_harness(AgentPreset::Codex),
            Some(RunnerCli::Codex)
        );
        assert_eq!(RunnerCli::of_harness(AgentPreset::Shell), None);
        assert_eq!(RunnerCli::Codex.login_args(), ["login", "--device-auth"]);
        assert_eq!(RunnerCli::Github.git_program(), Some("gh"));
        assert_eq!(RunnerCli::Claude.git_program(), None);
        assert!(RunnerLogin::SignedOut.needs_sign_in());
        assert!(!RunnerLogin::Missing.needs_sign_in());
    }
}
