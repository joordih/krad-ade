use gpui_kit::{App, Context};
use krad_domain::{CloudRunnerId, EnvironmentName, RunnerCli, RunnerLogin, TerminalId, ToastKind};
use krad_protocol::{Command, RunnerCommand};

use super::super::activity::{TerminalsTab, UtilityTab};
use super::super::live_terminal::{terminal_ref, LiveTerminal};
use super::super::local_reach::SERVICE_ONLY_TEXT;
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
        cx: &App,
    ) -> Option<RunnerLogin> {
        let shown = self.runtime.view.read(cx).runner_logins.get(&runner)?;
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
        cx: &App,
    ) -> Vec<(RunnerCli, RunnerLogin)> {
        RunnerCli::ALL
            .into_iter()
            .filter_map(|cli| Some((cli, self.runner_login(runner, cli, cx)?)))
            .collect()
    }

    pub(in crate::environment_shell) fn check_runner_logins(&self, runner: CloudRunnerId) {
        self.runtime
            .client
            .post(Command::Runner(RunnerCommand::CheckLogins { runner }));
    }

    pub(in crate::environment_shell) fn open_runner_sign_in(
        &mut self,
        runner: CloudRunnerId,
        cli: RunnerCli,
        cx: &mut Context<Self>,
    ) {
        if !self.runner_state(runner, cx).is_ready() {
            self.ensure_runner_ready(runner);
            return self.toast(ToastKind::Warning, NOT_READY, cx);
        }
        let label = format!(
            "{LABEL_PREFIX} {} · {}",
            self.runner_name(runner),
            cli_label(cli)
        );
        let Some((env, id)) = self.open_helper_terminal(label, cx) else {
            return;
        };
        let Some(handle) = self.runtime.local.handle() else {
            return self.push_toast(ToastKind::Warning, SERVICE_ONLY_TEXT.to_owned());
        };
        let view = handle.open_runner_sign_in(terminal_ref(&env, id), runner, cli);
        self.keep_live_session(&env, id, LiveTerminal::new(view, None), cx);
        self.show_utilities(UtilityTab::Terminals, cx);
        self.save_view();
        cx.notify();
    }

    pub(in crate::environment_shell) fn open_helper_terminal(
        &mut self,
        label: String,
        cx: &mut App,
    ) -> Option<(EnvironmentName, TerminalId)> {
        let env = self.active.clone();
        let id = self.alloc_terminal_id(&env, cx)?;
        let live = self.envs.get_mut(&env)?;
        live.view.side_panel.add_helper_terminal(id, label);
        live.terminals_tab = TerminalsTab::Shown;
        Some((env, id))
    }
}
