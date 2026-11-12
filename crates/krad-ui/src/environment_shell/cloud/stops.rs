use gpui_kit::{AppContext as _, Context};
use krad_chat::stop_cloud_process;
use krad_domain::{CloudProcessId, CloudRunnerId};

use super::super::EnvironmentShell;

pub(in crate::environment_shell) const REMOVED_RUNNER: &str = "This cloud runner was removed";

impl EnvironmentShell {
    pub(in crate::environment_shell) fn runner_removed_from_settings(
        &self,
        runner: CloudRunnerId,
    ) -> bool {
        self.settings.cloud.find(runner).is_none()
    }

    pub(super) fn queue_cloud_stop(
        &mut self,
        runner: CloudRunnerId,
        process: CloudProcessId,
        cx: &mut Context<Self>,
    ) {
        if self.runner_removed_from_settings(runner) {
            return;
        }
        tracing::debug!(%process, "the runner is away, so the stop waits for it");
        self.settings.cloud.queue_stop(runner, process);
        self.save_settings(cx);
        self.ensure_runner_ready(runner, cx);
    }

    pub(super) fn run_queued_stops(&mut self, runner: CloudRunnerId, cx: &mut Context<Self>) {
        let (Some(link), Some(cli), Some(pairing)) = (
            self.runner_link(runner),
            self.runner_cli(runner),
            self.runner_pairing(runner),
        ) else {
            return;
        };
        let stops = self.settings.cloud.take_stops(runner);
        if stops.is_empty() {
            return;
        }
        self.save_settings(cx);
        cx.background_spawn(async move {
            for process in stops {
                if let Err(error) = stop_cloud_process(&link, &cli, &pairing, &process) {
                    tracing::warn!(%process, %error, "a queued cloud process did not stop");
                }
            }
        })
        .detach();
    }

    pub(super) fn mark_runner_removed(&mut self, runner: CloudRunnerId) {
        for (env, id) in self.cloud_chats_on(runner) {
            if let Some(chat) = self
                .envs
                .get_mut(&env)
                .and_then(|live| live.chats.get_mut(&id))
            {
                chat.problem = Some(REMOVED_RUNNER.to_owned());
            }
        }
    }
}
