mod actions;
mod badge;
mod bus;
mod entry;
mod patch_dialog;
mod runners;
mod sessions;
mod sign_in;
mod state;
mod stops;
mod token;
mod tray;
mod webhooks;

pub(in crate::environment_shell) use actions::CloudAction;
pub(in crate::environment_shell) use badge::{cloud_marker, CloudBadge};
pub(in crate::environment_shell) use entry::CloudMenu;
pub(in crate::environment_shell) use krad_domain::RunnerState;
pub(in crate::environment_shell) use krad_domain::{RunnerCli, RunnerLogin};
pub(in crate::environment_shell) use sign_in::cli_label;
pub(in crate::environment_shell) use state::{linger_warning, runner_label};

use krad_domain::{CloudPlacement, CloudRunnerId, EnvironmentName, TerminalId};
use std::collections::HashSet;

use super::EnvironmentShell;

type NodeKey = (EnvironmentName, TerminalId);

#[derive(Default)]
pub(super) struct CloudRunners {
    asking: Option<krad_protocol::CloudDraftId>,
    helpers: HashSet<NodeKey>,
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn cloud_placement_of(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
    ) -> Option<CloudPlacement> {
        let data = &self.envs.get(env)?.data;
        data.session(data.session_of(id)?)?.cloud.clone()
    }

    pub(in crate::environment_shell) fn runner_name(&self, id: CloudRunnerId) -> String {
        self.settings
            .cloud
            .find(id)
            .map_or_else(|| id.to_string(), |runner| runner.name.as_str().to_owned())
    }

    pub(in crate::environment_shell) fn runner_state(&self, id: CloudRunnerId) -> RunnerState {
        self.runtime
            .view
            .runners
            .get(&id)
            .cloned()
            .unwrap_or_default()
    }
}
