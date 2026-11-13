mod actions;
mod badge;
mod bus;
mod entry;
mod opencode;
mod patch_dialog;
mod process;
mod runners;
mod sessions;
mod settle;
mod shells;
mod sign_in;
mod site;
mod state;
mod stops;
mod terminal;
mod token;
mod tray;
mod webhooks;

pub(in crate::environment_shell) use actions::CloudAction;
pub(in crate::environment_shell) use badge::{cloud_marker, CloudBadge};
pub(in crate::environment_shell) use entry::CloudMenu;
pub(in crate::environment_shell) use krad_domain::RunnerState;
pub(in crate::environment_shell) use krad_domain::{RunnerCli, RunnerLogin};
pub(in crate::environment_shell) use shells::CloudShell;
pub(in crate::environment_shell) use sign_in::cli_label;
pub(in crate::environment_shell) use site::{CloudSite, NodeSite};
pub(in crate::environment_shell) use state::{linger_warning, runner_label};

use krad_chat::CloudRun;
use krad_domain::{CloudRunnerId, EnvironmentName, Pairing, TerminalId};
use krad_host::HostLink;
use krad_runtime::ReadyRunner;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use self::process::{Liveness, LivenessCheck};
use super::EnvironmentShell;

type NodeKey = (EnvironmentName, TerminalId);

#[derive(Default)]
pub(super) struct CloudRunners {
    attached: HashMap<NodeKey, CloudRun>,
    reattached: HashSet<NodeKey>,
    asking: Option<krad_protocol::CloudDraftId>,
    opencode: HashMap<CloudRunnerId, opencode::CloudOpencode>,
    wanted: HashSet<CloudRunnerId>,
    liveness: HashMap<NodeKey, Liveness>,
    unchecked: Vec<LivenessCheck>,
    helpers: HashSet<NodeKey>,
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn runner_state(&self, id: CloudRunnerId) -> RunnerState {
        self.runtime
            .view
            .runners
            .get(&id)
            .cloned()
            .unwrap_or_default()
    }

    pub(in crate::environment_shell) fn runner_link(&self, id: CloudRunnerId) -> Option<HostLink> {
        self.ready_runner(id).map(|ready| ready.link)
    }

    pub(in crate::environment_shell) fn runner_pairing(
        &self,
        id: CloudRunnerId,
    ) -> Option<Pairing> {
        self.ready_runner(id).map(|ready| ready.pairing)
    }

    pub(in crate::environment_shell) fn runner_cli(&self, id: CloudRunnerId) -> Option<PathBuf> {
        self.ready_runner(id).map(|ready| ready.cli)
    }

    pub(in crate::environment_shell) fn ready_runner(
        &self,
        id: CloudRunnerId,
    ) -> Option<ReadyRunner> {
        self.runner_state(id)
            .is_ready()
            .then(|| self.runtime.handle.runner(id))
            .flatten()
    }
}
