use gpui_kit::Context;
use krad_domain::{CloudRunnerId, OfflineWebhooks};

use super::super::EnvironmentShell;

impl EnvironmentShell {
    pub(in crate::environment_shell) fn set_runner_offline_webhooks(
        &mut self,
        runner: CloudRunnerId,
        wanted: OfflineWebhooks,
        cx: &mut Context<Self>,
    ) {
        self.edit_settings(cx, |settings| {
            settings.cloud.set_offline_webhooks(runner, wanted)
        });
    }
}
