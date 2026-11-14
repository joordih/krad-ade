use gpui_kit::{App, Context};
use krad_domain::{ClaudeOauthToken, CloudRunnerId};
use krad_protocol::{Command, RunnerCommand};

use super::super::activity::UtilityTab;
use super::super::live_terminal::{terminal_ref, LiveTerminal};
use super::super::EnvironmentShell;

const SETUP_LABEL: &str = "Claude Code token";

impl EnvironmentShell {
    pub(in crate::environment_shell) fn runner_has_claude_token(
        &self,
        runner: CloudRunnerId,
        cx: &App,
    ) -> bool {
        self.runtime
            .view
            .read(cx)
            .runner_logins
            .get(&runner)
            .is_some_and(|shown| shown.claude_token)
    }

    pub(in crate::environment_shell) fn start_claude_setup_token(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some((env, id)) = self.open_helper_terminal(SETUP_LABEL.to_owned(), cx) else {
            return;
        };
        let view = self
            .runtime
            .handle
            .open_claude_setup_token(terminal_ref(&env, id));
        self.keep_live_session(&env, id, LiveTerminal::new(view, None), cx);
        self.show_utilities(UtilityTab::Terminals, cx);
        self.save_view();
        cx.notify();
    }

    pub(in crate::environment_shell) fn save_claude_token(
        &self,
        runner: CloudRunnerId,
        token: ClaudeOauthToken,
    ) {
        self.runtime
            .client
            .post(Command::Runner(RunnerCommand::KeepClaudeToken {
                runner,
                token,
            }));
    }

    pub(in crate::environment_shell) fn forget_claude_token(&self, runner: CloudRunnerId) {
        self.runtime
            .client
            .post(Command::Runner(RunnerCommand::ForgetClaudeToken { runner }));
    }
}
