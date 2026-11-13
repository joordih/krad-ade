use krad_agents::ApiFile;
use krad_protocol::{BusCommand, Command};
use krad_store::config_dir;

use super::super::EnvironmentShell;

impl EnvironmentShell {
    pub(in crate::environment_shell) fn sync_api_file(&self) {
        if self.runtime.handle.bus().is_none() {
            return;
        }
        self.runtime
            .client
            .post(Command::Bus(BusCommand::PublishDiscovery {
                file: self.settings.api_discovery_file,
            }));
    }

    pub(in crate::environment_shell) fn remove_api_file(&self) {
        if self.runtime.handle.bus().is_none() {
            return;
        }
        let Some(file) = config_dir().map(|dir| ApiFile::in_config(&dir)) else {
            return;
        };
        if let Err(error) = file.remove_if_written_by(std::process::id()) {
            tracing::warn!(%error, "agents API discovery file left behind");
        }
    }
}
