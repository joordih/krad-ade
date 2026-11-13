use gpui_kit::Context;
use krad_domain::{CloudRunnerId, EnvironmentName, RunnerCli, RunnerLogin, TerminalId, ToastKind};
use krad_protocol::{Command, RunnerCommand};

use super::super::activity::{TerminalsTab, UtilityTab};
use super::super::live_terminal::{terminal_ref, LiveTerminal};
use super::super::EnvironmentShell;

const LABEL_PREFIX: &str = "Sign in on";
const NOT_READY: &str = "The cloud runner is not connected yet; try again once it is ready";

pub(in crate::environment_shell) const fn cli_label(cli: RunnerCli) -> &'static str {
    match cli {
        RunnerCli::Claude => "Claude Code",
        RunnerCli::Codex => "Codex",
        RunnerCli::Opencode => "opencode",
        RunnerCli::Github => "GitHub CLI",
        RunnerCli::Gitlab => "GitLab CLI",
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn runner_login(
        &self,
        runner: CloudRunnerId,
        cli: RunnerCli,
    ) -> Option<RunnerLogin> {
        let shown = self.runtime.view.runner_logins.get(&runner)?;
        if cli == RunnerCli::Claude && shown.claude_token {
            return Some(RunnerLogin::SignedIn);
        }
        shown
            .logins
            .iter()
            .find(|known| known.cli == cli)
            .map(|known| known.login)
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

    pub(in crate::environment_shell) fn check_runner_logins(&self, runner: CloudRunnerId) {
        self.runtime
            .handle
            .post(Command::Runner(RunnerCommand::CheckLogins { runner }));
    }

    pub(in crate::environment_shell) fn open_runner_sign_in(
        &mut self,
        runner: CloudRunnerId,
        cli: RunnerCli,
        cx: &mut Context<Self>,
    ) {
        if self.runner_link(runner).is_none() {
            self.ensure_runner_ready(runner);
            return self.toast(ToastKind::Warning, NOT_READY, cx);
        }
        let label = format!(
            "{LABEL_PREFIX} {} · {}",
            self.runner_name(runner),
            cli_label(cli)
        );
        let Some((env, id)) = self.open_helper_terminal(label) else {
            return;
        };
        let view = self
            .runtime
            .handle
            .open_runner_sign_in(terminal_ref(&env, id), runner, cli);
        self.keep_live_session(&env, id, LiveTerminal::new(view, None));
        self.show_utilities(UtilityTab::Terminals, cx);
        self.store();
        cx.notify();
    }

    pub(in crate::environment_shell) fn open_helper_terminal(
        &mut self,
        label: String,
    ) -> Option<(EnvironmentName, TerminalId)> {
        let env = self.active.clone();
        let id = self.alloc_terminal_id(&env)?;
        let live = self.envs.get_mut(&env)?;
        live.data.side_panel_mut().add_terminal(id, label);
        live.terminals_tab = TerminalsTab::Shown;
        self.cloud.helpers.insert((env.clone(), id));
        Some((env, id))
    }

    pub(in crate::environment_shell) fn end_runner_helper(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
    ) {
        self.cloud.helpers.remove(&(env.clone(), id));
    }

    pub(in crate::environment_shell) fn runner_helper_terminals(
        &self,
        env: &EnvironmentName,
    ) -> Vec<TerminalId> {
        self.cloud
            .helpers
            .iter()
            .filter(|(owner, _)| owner == env)
            .map(|(_, id)| *id)
            .collect()
    }
}
