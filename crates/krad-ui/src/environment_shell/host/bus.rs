use gpui_kit::{AppContext as _, Context};
use krad_domain::{AgentPreset, EnvironmentName, ToastKind};
use krad_host::HostLink;

use super::super::agent_bus::{RemoteBus, RemoteBusPlan, RemoteBusSlot};
use super::super::EnvironmentShell;

const BUS_UNAVAILABLE: &str = "Agents on this host run without the agent bus";

impl EnvironmentShell {
    pub(super) fn prepare_remote_bus(&mut self, env: &EnvironmentName, cx: &mut Context<Self>) {
        let Some(plan) = self.remote_bus_plan(env) else {
            return;
        };
        if let Some(live) = self.envs.get_mut(env) {
            live.remote_bus = RemoteBusSlot::Preparing;
        }
        let prepare = cx.background_spawn(async move { plan.prepare() });
        let env = env.clone();
        cx.spawn(async move |this, cx| {
            let prepared = prepare.await;
            this.update(cx, |shell, cx| {
                match prepared {
                    Ok(bus) => shell.remote_bus_ready(&env, bus),
                    Err(error) => shell.remote_bus_unavailable(&env, &error.to_string()),
                }
                shell.remote_bus_settled(&env, cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn remote_bus_plan(&self, env: &EnvironmentName) -> Option<RemoteBusPlan> {
        let bus = self.agent_bus.as_ref()?;
        let live = self.envs.get(env)?;
        let HostLink::Remote(ssh) = &live.link else {
            return None;
        };
        Some(RemoteBusPlan::new(
            ssh.clone(),
            live.link.clone(),
            env.as_str(),
            bus.endpoint(),
            live.remote_bus_port,
        ))
    }

    fn remote_bus_ready(&mut self, env: &EnvironmentName, bus: RemoteBus) {
        if let Some(live) = self.envs.get_mut(env) {
            live.remote_bus_port = Some(bus.remote_port());
            live.remote_bus = RemoteBusSlot::Ready(bus);
        }
    }

    fn remote_bus_unavailable(&mut self, env: &EnvironmentName, reason: &str) {
        tracing::info!(env = env.as_str(), reason, "remote agent bus unavailable");
        let Some(live) = self.envs.get_mut(env) else {
            return;
        };
        live.remote_bus = RemoteBusSlot::Absent;
        let host = live.data.host.clone();
        let Some(bus) = &mut self.agent_bus else {
            return;
        };
        if !bus.remote_unavailable_told.insert(host) {
            return;
        }
        self.push_toast(ToastKind::Error, format!("{BUS_UNAVAILABLE}: {reason}"));
    }

    fn remote_bus_settled(&mut self, env: &EnvironmentName, cx: &mut Context<Self>) {
        self.start_deferred_launches(env);
        let harnesses: Vec<AgentPreset> =
            AgentPreset::listed(&self.settings.acp.all_agents()).collect();
        for harness in harnesses {
            self.flush_chats_of(harness, cx);
        }
        cx.notify();
    }

    pub(in crate::environment_shell) fn remote_bus(
        &self,
        env: &EnvironmentName,
    ) -> Option<&RemoteBus> {
        self.envs.get(env)?.remote_bus.ready()
    }

    pub(in crate::environment_shell) fn awaits_remote_bus(&self, env: &EnvironmentName) -> bool {
        self.envs
            .get(env)
            .is_some_and(|live| live.remote_bus.is_preparing())
    }
}
