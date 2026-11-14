use krad_domain::CloudRunnerId;

use super::super::EnvironmentShell;

pub(in crate::environment_shell) const REMOVED_RUNNER: &str = "This cloud runner was removed";

impl EnvironmentShell {
    pub(in crate::environment_shell) fn runner_removed_from_settings(
        &self,
        runner: CloudRunnerId,
    ) -> bool {
        self.runtime.settings.cloud.find(runner).is_none()
    }
}
