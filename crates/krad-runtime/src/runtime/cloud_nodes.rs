use krad_chat::stop_cloud_process;
use krad_domain::{CloudRunnerId, NodeEdit, NodeField, RunnerState};
use krad_protocol::AgentRef;
use std::time::{Duration, Instant};

use super::Runtime;
use crate::cloud_runs::{
    placement_of, saved_process, site_of, LivenessChecked, NodeSite, ProcessWrite, REMOVED_RUNNER,
};
use crate::opencode::CloudOpencodeStarted;

mod carried;
mod launching;

const RUNNER_PREFIX: &str = "Cloud runner";
const FAILED_PREFIX: &str = "Failed: ";
const CLOUD_OFFSETS_EVERY: Duration = Duration::from_secs(2);

impl Runtime {
    pub(super) fn settle_cloud_runs(&mut self) {
        for runner in self.cloud_runs.take_wanted() {
            let state = self.runners.state(runner);
            if matches!(state, RunnerState::NotConnected | RunnerState::Failed { .. }) {
                drop(self.ensure_runner(runner));
            }
        }
        for write in self.cloud_runs.take_writes() {
            self.keep_process(write);
        }
    }

    fn keep_process(&mut self, write: ProcessWrite) {
        let (node, process) = match write {
            ProcessWrite::Started(node, process) => (node, Some(process)),
            ProcessWrite::Offset(node, id, offset) => {
                let saved = saved_process(&self.environments, &node)
                    .filter(|saved| saved.id == id && saved.offset != offset);
                let Some(mut saved) = saved else {
                    return;
                };
                saved.offset = offset;
                (node, Some(saved))
            }
        };
        let kept = NodeEdit::Set {
            terminal: node.terminal,
            field: NodeField::Cloud(process),
        };
        self.environments
            .edit(node.environment, &kept.into(), None, &mut self.outbox);
    }

    pub(super) fn keep_cloud_offsets(&mut self, now: Instant) {
        if !self.cloud_runs.offsets_due(now, CLOUD_OFFSETS_EVERY) {
            return;
        }
        let writes: Vec<ProcessWrite> = self.cloud_runs.consumed().collect();
        for write in writes {
            self.keep_process(write);
        }
    }

    pub(super) fn cloud_runner_removed(&mut self, runner: CloudRunnerId) {
        self.opencode.cloud.forget(runner);
        self.chats
            .runner_problem(runner, REMOVED_RUNNER, &mut self.outbox);
    }

    pub(super) fn cloud_runner_returned(&mut self) {
        self.flush_chats();
        self.retry_cloud_launches();
    }

    pub(super) fn cloud_liveness_checked(&mut self, checked: LivenessChecked) {
        if !self.cloud_runs.checked(checked) {
            return;
        }
        self.flush_chats();
        self.retry_cloud_launches();
    }

    pub(super) fn cloud_opencode_started(&mut self, started: CloudOpencodeStarted) {
        let runner = started.runner;
        if self.opencode.cloud.started(started) {
            self.with_chat_work(|chats, work| chats.cloud_ready(runner, work));
        }
    }

    pub(super) fn cloud_runner_turned(&mut self, runner: CloudRunnerId, state: &RunnerState) {
        if let RunnerState::Failed { reason } = state {
            let name = self
                .settings
                .current
                .cloud
                .find(runner)
                .map_or_else(|| runner.to_string(), |found| found.name.as_str().to_owned());
            let problem = format!("{RUNNER_PREFIX} {name}: {FAILED_PREFIX}{reason}");
            self.chats.runner_problem(runner, &problem, &mut self.outbox);
        }
        if !state.is_ready() {
            self.opencode.cloud.forget(runner);
            return;
        }
        self.with_chat_work(|chats, work| chats.cloud_ready(runner, work));
        self.retry_cloud_launches();
    }

    #[tracing::instrument(skip_all, fields(environment = node.environment.as_str(), terminal = node.terminal.as_u64()))]
    pub(super) fn stop_cloud_node(&mut self, node: &AgentRef) {
        self.cloud_runs.forget(node);
        let Some(process) = saved_process(&self.environments, node) else {
            return;
        };
        let site = match site_of(placement_of(&self.environments, node), &self.runners) {
            NodeSite::Cloud(site) => site,
            NodeSite::Waiting(runner) => return self.queue_cloud_stop(runner, process.id),
            NodeSite::Env => return,
        };
        let started = self.jobs.run_reporting(move |_| {
            if let Err(error) =
                stop_cloud_process(&site.link, &site.cli, &site.pairing, &process.id)
            {
                tracing::warn!(process = %process.id, %error, "the cloud process did not stop");
            }
        });
        if let Err(error) = started {
            tracing::warn!(%error, "the cloud process stop could not start");
        }
    }

    fn queue_cloud_stop(&mut self, runner: CloudRunnerId, process: krad_domain::CloudProcessId) {
        if self.settings.current.cloud.find(runner).is_none() {
            return;
        }
        tracing::debug!(%process, "the runner is away, so the stop waits for it");
        self.settings
            .queue_cloud_stop(runner, process, &mut self.outbox);
        drop(self.ensure_runner(runner));
    }
}
