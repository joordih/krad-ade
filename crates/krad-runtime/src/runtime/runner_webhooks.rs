use krad_domain::CloudRunnerId;
use krad_protocol::EventsCommand;
use std::time::Instant;

use super::Runtime;
use crate::runners::runner_webhooks_plan;

impl Runtime {
    pub(super) fn sync_runner_webhooks(&mut self, now: Instant) {
        if !self.runners.webhooks.is_due(now) {
            return;
        }
        let runners: Vec<CloudRunnerId> = self
            .settings
            .current
            .cloud
            .runners
            .iter()
            .map(|runner| runner.id)
            .collect();
        for runner in runners {
            let Some(ready) = self.runners.directory.get(runner) else {
                continue;
            };
            let plan =
                runner_webhooks_plan(runner, &self.settings.current, self.environments.iter());
            let Some(write) = self.runners.webhooks.changed(runner, ready, plan) else {
                continue;
            };
            let secrets = self.secrets.clone();
            if let Err(error) = self.jobs.run_reporting(move |_| write.run(&secrets)) {
                tracing::warn!(%runner, %error, "the runner's webhooks sync could not start");
            }
        }
    }

    pub(super) fn webhooks_edited(&mut self, command: &EventsCommand) {
        if matches!(
            command,
            EventsCommand::UseWebhooks { .. }
                | EventsCommand::SaveWebhook { .. }
                | EventsCommand::RemoveWebhook { .. }
        ) {
            self.runners.webhooks.resync();
        }
    }
}
