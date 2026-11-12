use gpui_kit::{AppContext as _, Context};
use krad_chat::Runner;
use krad_domain::{
    AgentPreset, ClaudeOauthToken, CloudRunnerId, EnvironmentName, TerminalId, ToastKind,
};
use krad_host::HostLink;
use krad_store::{delete_cloud_claude_token, write_cloud_claude_token};

use super::super::activity::{TerminalsTab, UtilityTab};
use super::super::sessions::Launch;
use super::super::EnvironmentShell;
use super::{HelperTerminal, NodeSite};

const TOKEN_VAR: &str = "CLAUDE_CODE_OAUTH_TOKEN";
const SETUP_TOKEN_ARGS: [&str; 1] = ["setup-token"];
const SETUP_LABEL: &str = "Claude Code token";
const SAVED_PREFIX: &str = "New Claude Code chats on";
const SAVED_SUFFIX: &str = "use the long-lived token";
const FORGOTTEN: &str = "The long-lived Claude Code token was forgotten";
const NOT_SAVED: &str = "Could not keep the token";
const NOT_FORGOTTEN: &str = "Could not forget the token";

impl EnvironmentShell {
    pub(in crate::environment_shell) fn runner_claude_token(
        &self,
        runner: CloudRunnerId,
    ) -> Option<&ClaudeOauthToken> {
        self.cloud.claude_tokens.get(&runner)
    }

    pub(in crate::environment_shell) fn keep_runner_claude_token(
        &mut self,
        runner: CloudRunnerId,
        token: Option<ClaudeOauthToken>,
    ) {
        match token {
            Some(token) => self.cloud.claude_tokens.insert(runner, token),
            None => self.cloud.claude_tokens.remove(&runner),
        };
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
        let token = self.runner_claude_token(site.placement.runner)?;
        Some((TOKEN_VAR.to_owned(), token.expose().to_owned()))
    }

    pub(in crate::environment_shell) fn start_claude_setup_token(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let program = match self.harness_runner(&HostLink::Local, AgentPreset::ClaudeCode, None) {
            Ok((Runner::Native(program), _)) => program.to_string_lossy().into_owned(),
            Ok(_) => return,
            Err(error) => return self.toast(ToastKind::Warning, &error.to_string(), cx),
        };
        let env = self.active.clone();
        let Some(id) = self.alloc_terminal_id(&env) else {
            return;
        };
        if let Some(live) = self.envs.get_mut(&env) {
            live.data
                .side_panel
                .add_terminal(id, SETUP_LABEL.to_owned());
            live.terminals_tab = TerminalsTab::Shown;
        }
        self.cloud
            .helpers
            .insert((env.clone(), id), HelperTerminal::Plain);
        let launch = Launch::SignIn {
            program,
            args: SETUP_TOKEN_ARGS.map(str::to_owned).to_vec(),
        };
        self.start_session(&env, id, launch, None, &[]);
        self.show_utilities(UtilityTab::Terminals, cx);
        self.store();
        cx.notify();
    }

    pub(in crate::environment_shell) fn save_claude_token(
        runner: CloudRunnerId,
        token: ClaudeOauthToken,
        cx: &mut Context<Self>,
    ) {
        let kept = token.clone();
        let written = cx.background_spawn(async move { write_cloud_claude_token(runner, &kept) });
        cx.spawn(async move |this, cx| {
            let written = written.await;
            this.update(cx, |shell, cx| {
                if let Err(error) = written {
                    return shell.toast(ToastKind::Error, &format!("{NOT_SAVED}: {error}"), cx);
                }
                shell.keep_runner_claude_token(runner, Some(token));
                let text = format!(
                    "{SAVED_PREFIX} {} {SAVED_SUFFIX}",
                    shell.runner_name(runner)
                );
                shell.toast(ToastKind::Success, &text, cx);
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(in crate::environment_shell) fn forget_claude_token(
        runner: CloudRunnerId,
        cx: &mut Context<Self>,
    ) {
        let deleted = cx.background_spawn(async move { delete_cloud_claude_token(runner) });
        cx.spawn(async move |this, cx| {
            let deleted = deleted.await;
            this.update(cx, |shell, cx| {
                if let Err(error) = deleted {
                    return shell.toast(ToastKind::Error, &format!("{NOT_FORGOTTEN}: {error}"), cx);
                }
                shell.keep_runner_claude_token(runner, None);
                shell.toast(ToastKind::Success, FORGOTTEN, cx);
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }
}
