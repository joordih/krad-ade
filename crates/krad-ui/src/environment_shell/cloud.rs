mod actions;
mod badge;
mod bus;
mod carry;
mod catch_up;
mod entry;
mod install;
mod naming;
mod opencode;
mod patch_dialog;
mod process;
mod results;
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
mod workspace;

pub(in crate::environment_shell) use actions::CloudAction;
pub(in crate::environment_shell) use badge::{cloud_marker, CloudBadge};
pub(in crate::environment_shell) use entry::CloudMenu;
pub(in crate::environment_shell) use shells::CloudShell;
pub(in crate::environment_shell) use sign_in::{RunnerCli, RunnerLogin};
pub(in crate::environment_shell) use site::{CloudSite, NodeSite};
pub(in crate::environment_shell) use state::RunnerState;

use krad_chat::CloudRun;
use krad_domain::{ClaudeOauthToken, CloudRunnerId, EnvironmentName, Pairing, TerminalId};
use krad_host::HostLink;
use krad_session::PtyOpener;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;

use self::install::PreparedRunner;
use self::process::{Liveness, LivenessCheck};
use self::sessions::{PatchQuestion, PendingCloudSession};
use self::sign_in::{LoopbackCallback, SignInWatch};
use self::webhooks::WebhooksPlan;
use super::agent_bus::RemoteBusSlot;
use super::opencode::OpencodeLink;
use super::EnvironmentShell;

type NodeKey = (EnvironmentName, TerminalId);

enum HelperTerminal {
    SignIn(SignInWatch),
    Plain,
}

pub(in crate::environment_shell) struct LiveRunner {
    link: HostLink,
    state: RunnerState,
    prepared: Option<PreparedRunner>,
    bus: RemoteBusSlot,
    bus_port: Option<u16>,
}

#[derive(Default)]
pub(super) struct CloudRunners {
    live: HashMap<CloudRunnerId, LiveRunner>,
    attached: HashMap<NodeKey, CloudRun>,
    reattached: HashSet<NodeKey>,
    pending: Vec<PendingCloudSession>,
    asking: Option<PatchQuestion>,
    opencode: HashMap<CloudRunnerId, OpencodeLink>,
    wanted: HashSet<CloudRunnerId>,
    liveness: HashMap<NodeKey, Liveness>,
    unchecked: Vec<LivenessCheck>,
    logins: HashMap<(CloudRunnerId, RunnerCli), RunnerLogin>,
    claude_tokens: HashMap<CloudRunnerId, ClaudeOauthToken>,
    helpers: HashMap<NodeKey, HelperTerminal>,
    openers: HashMap<NodeKey, PtyOpener>,
    forward_asks: Vec<(NodeKey, LoopbackCallback)>,
    finished: Vec<SignInWatch>,
    heard: HashMap<NodeKey, Instant>,
    webhooks: HashMap<CloudRunnerId, WebhooksPlan>,
    webhooks_checked: Option<Instant>,
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn runner_state(&self, id: CloudRunnerId) -> RunnerState {
        self.cloud
            .live
            .get(&id)
            .map_or_else(RunnerState::default, |live| live.state.clone())
    }

    pub(in crate::environment_shell) fn runner_link(&self, id: CloudRunnerId) -> Option<HostLink> {
        let live = self.cloud.live.get(&id)?;
        live.state.is_ready().then(|| live.link.clone())
    }

    pub(in crate::environment_shell) fn runner_pairing(
        &self,
        id: CloudRunnerId,
    ) -> Option<Pairing> {
        self.ready_runner(id)
            .map(|prepared| prepared.pairing.clone())
    }

    pub(in crate::environment_shell) fn runner_cli(&self, id: CloudRunnerId) -> Option<PathBuf> {
        self.ready_runner(id).map(|prepared| prepared.cli.clone())
    }

    fn ready_runner(&self, id: CloudRunnerId) -> Option<&PreparedRunner> {
        let live = self.cloud.live.get(&id)?;
        live.state
            .is_ready()
            .then_some(live.prepared.as_ref())
            .flatten()
    }
}
