use gpui_kit::Context;
use krad_domain::{AgentPreset, ClaudeOauthToken, CloudRunnerId, EnvironmentName, TerminalId};
use krad_protocol::{Command, RunnerCommand};

use super::super::activity::UtilityTab;
use super::super::live_terminal::{terminal_ref, LiveTerminal};
use super::super::EnvironmentShell;
use super::NodeSite;

const TOKEN_VAR: &str = "CLAUDE_CODE_OAUTH_TOKEN";
const SETUP_LABEL: &str = "Claude Code token";

impl EnvironmentShell {
    pub(in crate::environment_shell) fn runner_has_claude_token(
        &self,
        runner: CloudRunnerId,
    ) -> bool {
        self.runtime
            .view
            .runner_logins
            .get(&runner)
            .is_some_and(|shown| shown.claude_token)
    }

    pub(in crate::environment_shell) fn cloud_claude_env(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
        harness: AgentPreset,
    ) -> Option<(String, String)> {
        if harness != AgentPreset::ClaudeCode {
            return None;
        }
        let NodeSite::Cloud(site) = self.node_site(env, id) else {
            return None;
        };
        let token = self.ready_runner(site.placement.runner)?.claude_token?;
        Some((TOKEN_VAR.to_owned(), token.expose().to_owned()))
    }

    pub(in crate::environment_shell) fn start_claude_setup_token(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some((env, id)) = self.open_helper_terminal(SETUP_LABEL.to_owned()) else {
            return;
        };
        let view = self
            .runtime
            .handle
            .open_claude_setup_token(terminal_ref(&env, id));
        self.keep_live_session(&env, id, LiveTerminal::new(view, None));
        self.show_utilities(UtilityTab::Terminals, cx);
        self.store();
        cx.notify();
    }

    pub(in crate::environment_shell) fn save_claude_token(
        &self,
        runner: CloudRunnerId,
        token: ClaudeOauthToken,
    ) {
        self.runtime
            .handle
            .post(Command::Runner(RunnerCommand::KeepClaudeToken {
                runner,
                token,
            }));
    }

    pub(in crate::environment_shell) fn forget_claude_token(&self, runner: CloudRunnerId) {
        self.runtime
            .handle
            .post(Command::Runner(RunnerCommand::ForgetClaudeToken { runner }));
    }
}
