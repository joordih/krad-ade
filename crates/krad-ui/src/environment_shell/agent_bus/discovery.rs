use krad_protocol::{BusCommand, Command};

use super::super::EnvironmentShell;

impl EnvironmentShell {
    pub(in crate::environment_shell) fn sync_api_file(&self) {
        if self.runtime.handle.bus().is_none() {
            return;
        }
        self.runtime
            .client
            .post(Command::Bus(BusCommand::PublishDiscovery {
                file: self.runtime.settings.api_discovery_file,
            }));
    }

    pub(in crate::environment_shell) fn remove_api_file(&self) {
        self.runtime.handle.withdraw_discovery();
    }
}
