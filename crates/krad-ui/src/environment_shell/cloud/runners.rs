use gpui_kit::{App, Context};
use krad_domain::{CloudRunnerId, CloudRunnerName, RunnerState, SshDestination, ToastKind};
use krad_protocol::{Command, RunnerCommand, RunnerNews, RunnerNotice, RunnerRemoved};

use super::super::EnvironmentShell;

const ADDED_TOAST: &str = "Cloud runner added";
const REMOVED_TOAST: &str = "Cloud runner removed";
const TOKEN_SAVED_PREFIX: &str = "New Claude Code chats on";
const TOKEN_SAVED_SUFFIX: &str = "use the long-lived token";
const TOKEN_FORGOTTEN: &str = "The long-lived Claude Code token was forgotten";
const TOKEN_NOT_SAVED: &str = "Could not keep the token";
const TOKEN_NOT_FORGOTTEN: &str = "Could not forget the token";
const FORWARDING_PREFIX: &str = "The browser can now finish the sign-in through";

impl EnvironmentShell {
    fn post_runner(&self, command: RunnerCommand) {
        self.runtime.client.post(Command::Runner(command));
    }

    pub(in crate::environment_shell) fn ensure_runner_ready(&self, runner: CloudRunnerId) {
        self.post_runner(RunnerCommand::Ensure { runner });
    }

    pub(in crate::environment_shell) fn show_cloud_runners(&self, cx: &App) {
        for runner in &self.settings.cloud.runners {
            if self.runner_state(runner.id, cx) == RunnerState::NotConnected {
                self.ensure_runner_ready(runner.id);
            }
        }
    }

    pub(in crate::environment_shell) fn reconnect_runner(&self, runner: CloudRunnerId) {
        self.post_runner(RunnerCommand::Reconnect { runner });
    }

    pub(in crate::environment_shell) fn add_cloud_runner(
        &mut self,
        name: CloudRunnerName,
        destination: SshDestination,
        cx: &mut Context<Self>,
    ) {
        let id = self.settings.cloud.add(name, destination);
        self.save_settings(cx);
        self.toast(ToastKind::Success, ADDED_TOAST, cx);
        self.ensure_runner_ready(id);
    }

    pub(in crate::environment_shell) fn remove_cloud_runner(
        &mut self,
        runner: CloudRunnerId,
        cx: &mut Context<Self>,
    ) {
        if self.settings.cloud.remove(runner).is_none() {
            return;
        }
        self.save_settings(cx);
        self.runtime.view.update(cx, |model, _| {
            model.runner_logins.remove(&runner);
            model.runners.remove(&runner);
            model.remotes.retain(|kept| kept != runner);
        });
        self.post_runner(RunnerCommand::Remove { runner });
        cx.notify();
    }

    pub(in crate::environment_shell) fn runner_changed(
        &mut self,
        runner: CloudRunnerId,
        cx: &mut Context<Self>,
    ) {
        if self.runner_removed_from_settings(runner) {
            self.runtime.view.update(cx, |model, _| {
                model.runners.remove(&runner);
                model.remotes.retain(|kept| kept != runner);
            });
        }
    }

    pub(in crate::environment_shell) fn runner_removed(
        &mut self,
        removed: RunnerRemoved,
        cx: &mut Context<Self>,
    ) {
        match removed.problem {
            Some(problem) => self.toast(ToastKind::Error, &problem, cx),
            None => self.toast(ToastKind::Success, REMOVED_TOAST, cx),
        }
    }

    pub(in crate::environment_shell) fn runner_news(
        &mut self,
        notice: RunnerNotice,
        cx: &mut Context<Self>,
    ) {
        let (kind, text) = match notice.news {
            RunnerNews::ClaudeTokenKept => (
                ToastKind::Success,
                format!(
                    "{TOKEN_SAVED_PREFIX} {} {TOKEN_SAVED_SUFFIX}",
                    self.runner_name(notice.runner)
                ),
            ),
            RunnerNews::ClaudeTokenForgotten => (ToastKind::Success, TOKEN_FORGOTTEN.to_owned()),
            RunnerNews::ClaudeTokenNotKept { error } => {
                (ToastKind::Error, format!("{TOKEN_NOT_SAVED}: {error}"))
            }
            RunnerNews::ClaudeTokenNotForgotten { error } => {
                (ToastKind::Error, format!("{TOKEN_NOT_FORGOTTEN}: {error}"))
            }
            RunnerNews::SignInForwarded { url } => {
                (ToastKind::Info, format!("{FORWARDING_PREFIX} {url}"))
            }
            RunnerNews::SignInNotForwarded { message } => (ToastKind::Warning, message),
            RunnerNews::Unknown => return,
        };
        self.toast(kind, &text, cx);
    }
}
