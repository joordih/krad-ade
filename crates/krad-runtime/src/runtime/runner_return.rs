use krad_agents::{queued_events, QueuedEvent, Request};
use krad_domain::{
    AttentionKind, CloudProcessId, CloudRunnerId, EnvironmentData, ToolKind, WebhookId,
};
use krad_host::runner::take_events_command;
use krad_host::{HostCommand, HostLink};
use krad_protocol::{AgentRef, EventKind, HookOrigin, HookReport};
use std::collections::BTreeSet;
use std::time::Instant;

use super::hooking::hook_report;
use super::Runtime;
use crate::hosts::RemoteBusPlan;
use crate::runners::{RunnerDone, RunnerReturn};

#[derive(Debug, PartialEq, Eq)]
enum Replay {
    Hook(HookReport),
    Attention(AttentionKind),
}

fn ask_attention(tool: &str) -> AttentionKind {
    match ToolKind::of_claude_code(tool) {
        ToolKind::Question | ToolKind::Plan => AttentionKind::Ask,
        _ => AttentionKind::Permission,
    }
}

fn replay_of(request: Request) -> Option<Replay> {
    match request {
        Request::Ask { tool, .. } => Some(Replay::Attention(ask_attention(&tool))),
        hook @ Request::Hook { .. } => hook_report(hook).map(Replay::Hook),
        _ => None,
    }
}

fn node_of_process<'a>(
    environments: impl IntoIterator<Item = &'a EnvironmentData>,
    process: &CloudProcessId,
) -> Option<AgentRef> {
    environments.into_iter().find_map(|data| {
        data.nodes
            .iter()
            .find(|node| {
                node.cloud
                    .as_ref()
                    .is_some_and(|cloud| cloud.id == *process)
            })
            .map(|node| AgentRef {
                environment: data.name.clone(),
                terminal: node.id,
            })
    })
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

impl Runtime {
    #[tracing::instrument(skip_all, fields(%runner))]
    pub(super) fn start_runner_return(&mut self, runner: CloudRunnerId) {
        let Some(ready) = self.runners.directory.get(runner) else {
            return;
        };
        let HostLink::Remote(ssh) = &ready.link else {
            return;
        };
        let take = ssh
            .layout()
            .map(|layout| take_events_command(&layout.runner_dir(), &ready.pairing.install));
        let port = self.runners.buses.port(runner);
        let plan = self.bus.as_ref().map(|bus| {
            RemoteBusPlan::for_runner(
                ssh.clone(),
                ready.link.clone(),
                runner,
                &bus.access().endpoint,
                port,
            )
        });
        if plan.is_some() {
            let shown = self.runners.buses.preparing(runner);
            self.outbox.push(EventKind::RunnerBusChanged(shown));
        }
        let started = Instant::now();
        let link = ready.link;
        let run = self.jobs.run(move || {
            let events = take_events(&link, take);
            let bus = plan.map(RemoteBusPlan::prepare);
            RunnerDone::Returned(Box::new(RunnerReturn {
                runner,
                started,
                events,
                bus,
            }))
            .into()
        });
        if let Err(error) = run {
            tracing::warn!(%error, "the cloud runner's return could not start");
            let shown = self.runners.buses.prepared(runner, None);
            self.outbox.push(EventKind::RunnerBusChanged(shown));
        }
    }

    pub(super) fn runner_returned(&mut self, back: RunnerReturn) {
        let RunnerReturn {
            runner,
            started,
            events,
            bus,
        } = back;
        if !self.runners.state(runner).is_ready() {
            return;
        }
        let shown = self.runners.buses.prepared(runner, bus);
        self.outbox.push(EventKind::RunnerBusChanged(shown));
        self.catch_up(events, started);
        self.outbox.push(EventKind::RunnerReturned { runner });
        self.cloud_runner_returned();
    }

    fn catch_up(&mut self, events: Vec<QueuedEvent>, started: Instant) {
        for event in events {
            let Some(agent) = node_of_process(self.environments.iter(), &event.process) else {
                continue;
            };
            if self.runners.buses.heard_since(&agent, started) {
                continue;
            }
            let Some(replay) = replay_of(event.request) else {
                continue;
            };
            self.replay(agent, replay, event.webhooked);
        }
    }

    fn replay(&mut self, agent: AgentRef, replay: Replay, delivered: BTreeSet<WebhookId>) {
        match replay {
            Replay::Hook(report) => {
                self.hear_hook(agent, report, HookOrigin::Replayed { delivered });
            }
            Replay::Attention(kind) => self.raise_attention(agent, kind),
        }
    }

    pub(super) fn note_live_cloud_report(&mut self, agent: &AgentRef) {
        let cloud = self
            .environments
            .data(&agent.environment)
            .and_then(|data| data.nodes.iter().find(|node| node.id == agent.terminal))
            .is_some_and(|node| node.cloud.is_some());
        if cloud {
            self.runners.buses.hear(agent.clone(), Instant::now());
        }
    }

    pub(super) fn watch_runner_buses(&mut self) {
        for shown in self.runners.buses.lost() {
            let runner = shown.runner;
            self.outbox.push(EventKind::RunnerBusChanged(shown));
            let linked = self
                .runners
                .directory
                .get(runner)
                .is_some_and(|ready| ready.link.is_ready());
            if linked {
                self.start_runner_return(runner);
            }
        }
    }
}

#[cfg(test)]
mod tests;
