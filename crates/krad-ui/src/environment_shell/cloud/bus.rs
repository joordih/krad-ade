use gpui_kit::{AppContext as _, Context};
use krad_agents::{queued_events, Caller, QueuedEvent, TerminalHandle};
use krad_domain::{CloudRunnerId, EnvironmentName, TerminalId};
use krad_host::runner::take_events_command;
use krad_host::{HostCommand, HostLink};
use std::time::Instant;

use super::super::agent_bus::{RemoteBus, RemoteBusPlan, RemoteBusSlot};
use super::super::host_cli::HostCliError;
use super::super::EnvironmentShell;
use super::NodeSite;

struct RunnerReturn {
    events: Vec<QueuedEvent>,
    bus: Option<Result<RemoteBus, HostCliError>>,
}

fn take_events(link: &HostLink, command: Option<HostCommand>) -> Vec<QueuedEvent> {
    let Some(command) = command else {
        return Vec::new();
    };
    match link.output(&command) {
        Ok(output) if output.status.success() => {
            queued_events(&String::from_utf8_lossy(&output.stdout))
        }
        Ok(output) => {
            tracing::warn!(status = ?output.status.code(), "the runner's event queue could not be read");
            Vec::new()
        }
        Err(error) => {
            tracing::warn!(%error, "the runner's event queue could not be read");
            Vec::new()
        }
    }
}

impl EnvironmentShell {
    fn runner_bus_plan(&self, runner: CloudRunnerId) -> Option<RemoteBusPlan> {
        let bus = self.agent_bus.as_ref()?;
        let live = self.cloud.live.get(&runner)?;
        let HostLink::Remote(ssh) = &live.link else {
            return None;
        };
        Some(RemoteBusPlan::for_runner(
            ssh.clone(),
            live.link.clone(),
            runner,
            bus.endpoint(),
            live.bus_port,
        ))
    }

    pub(super) fn start_runner_return(&mut self, runner: CloudRunnerId, cx: &mut Context<Self>) {
        let Some(link) = self.runner_link(runner) else {
            return;
        };
        let take = match (&link, self.runner_pairing(runner)) {
            (HostLink::Remote(ssh), Some(pairing)) => ssh
                .layout()
                .map(|layout| take_events_command(&layout.runner_dir(), &pairing.install)),
            _ => None,
        };
        let plan = self.runner_bus_plan(runner);
        if let Some(live) = self.cloud.live.get_mut(&runner) {
            live.bus = if plan.is_some() {
                RemoteBusSlot::Preparing
            } else {
                RemoteBusSlot::Absent
            };
        }
        let started = Instant::now();
        let back = cx.background_spawn(async move {
            let events = take_events(&link, take);
            let bus = plan.map(RemoteBusPlan::prepare);
            RunnerReturn { events, bus }
        });
        cx.spawn(async move |this, cx| {
            let back = back.await;
            this.update(cx, |shell, cx| {
                shell.runner_returned(runner, started, back, cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn runner_returned(
        &mut self,
        runner: CloudRunnerId,
        started: Instant,
        back: RunnerReturn,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.cloud.live.get_mut(&runner) {
            live.bus = match back.bus {
                Some(Ok(bus)) => {
                    live.bus_port = Some(bus.remote_port());
                    RemoteBusSlot::Ready(bus)
                }
                Some(Err(error)) => {
                    tracing::info!(%runner, %error, "the cloud runner's agent bus is unavailable");
                    RemoteBusSlot::Absent
                }
                None => RemoteBusSlot::Absent,
            };
        }
        self.catch_up(back.events, started);
        self.resume_cloud_work(cx);
    }

    pub(in crate::environment_shell) fn watch_runner_buses(&mut self, cx: &mut Context<Self>) {
        let dropped: Vec<CloudRunnerId> = self
            .cloud
            .live
            .iter_mut()
            .filter_map(|(id, live)| {
                let RemoteBusSlot::Ready(bus) = &mut live.bus else {
                    return None;
                };
                if bus.is_alive() {
                    return None;
                }
                live.bus = RemoteBusSlot::Absent;
                Some(*id)
            })
            .collect();
        for runner in dropped {
            if self.runner_link(runner).is_some_and(|link| link.is_ready()) {
                self.start_runner_return(runner, cx);
            }
        }
    }

    pub(super) fn drop_runner_bus(&mut self, runner: CloudRunnerId) {
        if let Some(live) = self.cloud.live.get_mut(&runner) {
            live.bus = RemoteBusSlot::Absent;
        }
    }

    fn runner_bus(&self, runner: CloudRunnerId) -> &RemoteBusSlot {
        self.cloud
            .live
            .get(&runner)
            .map_or(&RemoteBusSlot::Absent, |live| &live.bus)
    }

    pub(in crate::environment_shell) fn awaits_runner_bus(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
    ) -> bool {
        self.cloud_placement_of(env, id)
            .is_some_and(|placement| self.runner_bus(placement.runner).is_preparing())
    }

    fn ready_runner_bus(&self, env: &EnvironmentName, id: TerminalId) -> Option<&RemoteBus> {
        match self.node_site(env, id) {
            NodeSite::Cloud(site) => self.runner_bus(site.placement.runner).ready(),
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
            .map_or_default(|bus| bus.detached_caller_vars(&caller))
    }

    pub(in crate::environment_shell) fn cloud_hook_settings(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
    ) -> Option<String> {
        self.ready_runner_bus(env, id)
            .map(|bus| bus.claude_settings.to_string_lossy().into_owned())
    }

    pub(in crate::environment_shell) fn note_live_cloud_report(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
    ) {
        if self.cloud_placement_of(env, id).is_some() {
            self.cloud.heard.insert((env.clone(), id), Instant::now());
        }
    }
}
