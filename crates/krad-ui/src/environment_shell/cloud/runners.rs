use gpui_kit::Context;
use krad_domain::{CloudRunnerId, CloudRunnerName, RunnerState, SshDestination, ToastKind};
use krad_protocol::{Command, RunnerCommand, RunnerRemoved, RunnerShown};

use super::super::EnvironmentShell;

const ADDED_TOAST: &str = "Cloud runner added";
const REMOVED_TOAST: &str = "Cloud runner removed";

impl EnvironmentShell {
    fn post_runner(&self, command: RunnerCommand) {
        self.runtime.handle.post(Command::Runner(command));
    }

    pub(in crate::environment_shell) fn ensure_runner_ready(&self, runner: CloudRunnerId) {
        self.post_runner(RunnerCommand::Ensure { runner });
    }

    pub(in crate::environment_shell) fn show_cloud_runners(&self) {
        for runner in &self.settings.cloud.runners {
            if self.runner_state(runner.id) == RunnerState::NotConnected {
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
        self.forget_runner_logins(runner);
        self.forget_runner_webhooks(runner);
        self.keep_runner_claude_token(runner, None);
        self.mark_runner_removed(runner);
        self.drop_runner_bus(runner);
        self.runtime.view.runners.remove(&runner);
        self.post_runner(RunnerCommand::Remove { runner });
        cx.notify();
    }

    pub(in crate::environment_shell) fn runner_changed(
        &mut self,
        shown: RunnerShown,
        cx: &mut Context<Self>,
    ) {
        let RunnerShown { runner, state } = shown;
        if self.runner_removed_from_settings(runner) {
            return;
        }
        let settled = state.is_ready() || matches!(state, RunnerState::Failed { .. });
        if state.is_ready() {
            let token = self
                .runtime
                .handle
                .runner(runner)
                .and_then(|ready| ready.claude_token);
            self.keep_runner_claude_token(runner, token);
        } else {
            self.drop_runner_bus(runner);
        }
        self.runtime.view.runners.insert(runner, state);
        if settled {
            self.cloud_runner_settled(runner, cx);
        }
        cx.notify();
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
}
