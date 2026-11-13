use krad_domain::EnvironmentName;
use krad_host::MachineChannel;
use krad_protocol::{AgentRef, EnvironmentCommand, EventKind, HostCommand, Reply, TerminalSummary};
use std::sync::Arc;
use std::time::Instant;

use super::Runtime;
use crate::hosts::HostWork;
use crate::jobs::JobDone;
use crate::terminals::Shared;

impl Runtime {
    fn host_work(&mut self) -> (&mut crate::hosts::Hosts, HostWork<'_>) {
        (
            &mut self.hosts,
            HostWork {
                jobs: &self.jobs,
                terminals: &mut self.terminals,
                outbox: &mut self.outbox,
            },
        )
    }

    pub(super) fn execute_host(&mut self, command: HostCommand) -> Option<Reply> {
        let host = match &command {
            HostCommand::Show { environment } | HostCommand::Reconnect { environment } => {
                self.environments.host_of(environment)
            }
            HostCommand::AnswerPrompt { .. } | HostCommand::Unknown => None,
        };
        let (hosts, mut work) = self.host_work();
        hosts.execute(command, host, &mut work)
    }

    pub(super) fn execute_environments(&mut self, command: EnvironmentCommand) -> Option<Reply> {
        let closed = match &command {
            EnvironmentCommand::Close { environment } => self
                .environments
                .host_of(environment)
                .map(|host| (environment.clone(), host)),
            _ => None,
        };
        let reply = self.environments.execute(command, &mut self.outbox)?;
        if let (Some((environment, _)), Reply::Accepted) = (&closed, &reply) {
            self.chats
                .close_environment(environment.as_str(), &mut self.outbox);
        }
        if let (Some((environment, host)), Reply::Accepted) = (closed, &reply) {
            let still_used = self.environments.uses_host(&host);
            self.hosts.environment_closed(&environment, still_used);
        }
        Some(reply)
    }

    pub(super) fn tick_hosts(&mut self) {
        let (hosts, mut work) = self.host_work();
        hosts.tick(Instant::now(), &mut work);
    }

    pub(super) fn open_sign_in(&mut self, terminal: AgentRef, shared: Arc<Shared>) {
        let (hosts, mut work) = self.host_work();
        hosts.open_sign_in(terminal, shared, &mut work);
    }

    pub(super) fn hand_machine_channel(
        &mut self,
        environment: &EnvironmentName,
        channel: Result<MachineChannel, String>,
    ) {
        let (hosts, mut work) = self.host_work();
        hosts.machine_channel(environment, channel, &mut work);
    }

    pub(super) fn host_job_done(&mut self, done: JobDone) {
        match done {
            JobDone::HostConnected {
                environment,
                host,
                attempt,
                outcome,
            } => {
                let (hosts, mut work) = self.host_work();
                hosts.connected(&environment, (host, attempt), outcome, &mut work);
            }
            JobDone::RemoteBusPrepared {
                environment,
                host,
                prepared,
            } => self
                .hosts
                .bus_prepared(&environment, &host, prepared, &mut self.outbox),
            JobDone::ChatOpened(opening) => self.chat_opened(opening),
            JobDone::HarnessChecked { .. }
            | JobDone::ModelsListed { .. }
            | JobDone::UsageRead { .. } => {}
        }
    }

    pub(super) fn hear_ended_terminals(&mut self, unread: usize) {
        let ended: Vec<TerminalSummary> = self
            .outbox
            .iter()
            .skip(unread)
            .filter_map(|event| match event {
                EventKind::TerminalEnded(summary) => Some(summary.clone()),
                _ => None,
            })
            .collect();
        if ended.is_empty() {
            return;
        }
        let (hosts, mut work) = self.host_work();
        hosts.terminals_ended(&ended, &mut work);
    }
}
