use krad_agents::{Caller, TerminalHandle};
use krad_domain::{CloudRunnerId, EnvironmentName, TerminalId};
use krad_protocol::{CloudAskReplayed, RemoteBusInfo, RemoteBusState};
use krad_runtime::remote_detached_caller_vars;

use super::super::EnvironmentShell;
use super::NodeSite;

impl EnvironmentShell {
    fn runner_bus(&self, runner: CloudRunnerId) -> Option<&RemoteBusState> {
        self.runtime.view.runner_buses.get(&runner)
    }

    pub(in crate::environment_shell) fn awaits_runner_bus(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
    ) -> bool {
        self.cloud_placement_of(env, id).is_some_and(|placement| {
            self.runner_bus(placement.runner)
                .is_some_and(RemoteBusState::is_preparing)
        })
    }

    fn ready_runner_bus(&self, env: &EnvironmentName, id: TerminalId) -> Option<&RemoteBusInfo> {
        match self.node_site(env, id) {
            NodeSite::Cloud(site) => self
                .runner_bus(site.placement.runner)
                .and_then(RemoteBusState::ready),
            NodeSite::Env | NodeSite::Waiting(_) => None,
        }
    }

    pub(in crate::environment_shell) fn cloud_bus_vars(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
    ) -> Vec<(String, String)> {
        let caller = Caller::Terminal(TerminalHandle {
            environment: env.as_str().to_owned(),
            terminal: id.as_u64(),
        });
        self.ready_runner_bus(env, id)
            .map_or_default(|bus| remote_detached_caller_vars(bus, &caller))
    }

    pub(in crate::environment_shell) fn cloud_hook_settings(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
    ) -> Option<String> {
        self.ready_runner_bus(env, id)
            .map(|bus| bus.claude_settings.to_string_lossy().into_owned())
    }

    pub(in crate::environment_shell) fn cloud_ask_replayed(&mut self, asked: CloudAskReplayed) {
        let CloudAskReplayed {
            agent,
            kind,
            delivered,
        } = asked;
        self.with_webhooks_delivered(delivered, |shell| {
            shell.raise_attention(&agent.environment, agent.terminal, kind);
        });
    }
}
