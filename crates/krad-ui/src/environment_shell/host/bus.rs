use futures::future::{select, Either};
use gpui_kit::{AppContext as _, Context};
use krad_domain::EnvironmentName;
use krad_host::HostLink;
use std::time::Duration;

use super::super::agent_bus::{RemoteBus, RemoteBusPlan};
use super::super::EnvironmentShell;

const BUS_UNAVAILABLE: &str = "Agents on this host run without the agent bus";
const BUS_SLOW: &str = "the bus is still being prepared; agents started meanwhile run without it";
const BUS_WAIT: Duration = Duration::from_secs(15);

impl EnvironmentShell {
    pub(super) fn prepare_remote_bus(
        &mut self,
        env: &EnvironmentName,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(plan) = self.remote_bus_plan(env) else {
            return false;
        };
        let prepare = cx.background_spawn(async move { plan.prepare() });
        let waited = cx.background_executor().timer(BUS_WAIT);
        let env = env.clone();
        cx.spawn(async move |this, cx| {
            let prepared = match select(prepare, waited).await {
                Either::Left((prepared, _)) => prepared,
                Either::Right(((), prepare)) => {
                    this.update(cx, |shell, cx| {
                        shell.remote_bus_unavailable(&env, BUS_SLOW);
                        shell.finish_host_connection(&env, cx);
                    })
                    .unwrap_or_default();
                    let prepared = prepare.await;
                    this.update(cx, |shell, _| match prepared {
                        Ok(bus) => shell.remote_bus_ready(&env, bus),
                        Err(error) => shell.remote_bus_unavailable(&env, &error.to_string()),
                    })
                    .unwrap_or_default();
                    return;
                }
            };
            this.update(cx, |shell, cx| {
                match prepared {
                    Ok(bus) => shell.remote_bus_ready(&env, bus),
                    Err(error) => shell.remote_bus_unavailable(&env, &error.to_string()),
                }
                shell.finish_host_connection(&env, cx);
            })
            .unwrap_or_default();
        })
        .detach();
        true
    }

    fn remote_bus_plan(&self, env: &EnvironmentName) -> Option<RemoteBusPlan> {
        let bus = self.agent_bus.as_ref()?;
        let live = self.envs.get(env)?;
        let HostLink::Ssh(ssh) = &live.link else {
            return None;
        };
        Some(RemoteBusPlan::new(
            ssh.clone(),
            live.link.clone(),
            env.as_str(),
            bus.endpoint(),
        ))
    }

    fn remote_bus_ready(&mut self, env: &EnvironmentName, bus: RemoteBus) {
        if let Some(live) = self.envs.get_mut(env) {
            live.remote_bus = Some(bus);
        }
    }

    fn remote_bus_unavailable(&mut self, env: &EnvironmentName, reason: &str) {
        tracing::info!(env = env.as_str(), reason, "remote agent bus unavailable");
        let Some(host) = self.envs.get(env).map(|live| live.data.host.clone()) else {
            return;
        };
        let Some(bus) = &mut self.agent_bus else {
            return;
        };
        if !bus.remote_unavailable_told.insert(host) {
            return;
        }
        self.push_toast(format!("{BUS_UNAVAILABLE}: {reason}"));
    }

    pub(in crate::environment_shell) fn remote_bus(
        &self,
        env: &EnvironmentName,
    ) -> Option<&RemoteBus> {
        self.envs.get(env)?.remote_bus.as_ref()
    }
}
