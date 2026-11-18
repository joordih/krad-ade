use krad_domain::EnvironmentName;
use krad_host::MachineChannel;
use krad_protocol::{
    AgentRef, EnvironmentCommand, ErrorCode, EventKind, HostCommand, LaunchedAs, Reply, RequestId,
    TerminalLaunched, TerminalSummary,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use super::{refused, Runtime};
use crate::gateway::Gateway;
use crate::hosts::{HostWork, Hosts};
use crate::jobs::JobDone;
use crate::terminals::Shared;

const UNKNOWN_ENVIRONMENT: &str = "this runtime has no environment of that name";

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
        if let HostCommand::OpenSignIn { terminal } = command {
            self.open_watched_sign_in(terminal);
            return Some(Reply::Accepted);
        }
        let host = match &command {
            HostCommand::Show { environment } | HostCommand::Reconnect { environment } => {
                self.environments.host_of(environment)
            }
            HostCommand::DetectAgents { environment } => {
                if !self.environments.knows(environment) {
                    return Some(refused(ErrorCode::NotFound, UNKNOWN_ENVIRONMENT));
                }
                self.detect_host_agents(environment);
                return Some(Reply::Accepted);
            }
            HostCommand::DetectLocalAgents { place } => {
                Hosts::detect_local_agents(
                    place.clone(),
                    self.settings.current.acp.all_agents(),
                    &self.jobs,
                );
                return Some(Reply::Accepted);
            }
            HostCommand::CheckAcpAdapter { agent } => {
                if let Some(agent) = self.settings.current.acp.agent(*agent) {
                    self.adapters.check(&agent, &self.jobs, &mut self.outbox);
                }
                return Some(Reply::Accepted);
            }
            HostCommand::RetryAcpAdapter { agent } => {
                self.adapters.retry(*agent, &mut self.outbox);
                self.flush_chats();
                return Some(Reply::Accepted);
            }
            HostCommand::CloseAll => {
                self.hosts.close_all();
                return Some(Reply::Accepted);
            }
            HostCommand::AnswerPrompt { prompt, answer } => {
                let asked = self.hosts.prompt_asker(*prompt);
                let given = answer.is_some();
                let (hosts, mut work) = self.host_work();
                let reply = hosts.execute(command, None, &mut work);
                self.runner_prompt_answered(asked.as_ref(), given);
                return reply;
            }
            HostCommand::OpenSignIn { .. }
            | HostCommand::SshHosts
            | HostCommand::AcpPackages { .. }
            | HostCommand::InstallAcpPackage { .. }
            | HostCommand::Unknown => None,
        };
        let (hosts, mut work) = self.host_work();
        hosts.execute(command, host, &mut work)
    }

    pub(super) fn execute_environments(
        &mut self,
        command: EnvironmentCommand,
        request: RequestId,
    ) -> Option<Reply> {
        let closed = match &command {
            EnvironmentCommand::Close { environment } => self
                .environments
                .host_of(environment)
                .map(|host| (environment.clone(), host)),
            _ => None,
        };
        let roots: Vec<PathBuf> = closed
            .as_ref()
            .and_then(|(environment, _)| self.environments.data(environment))
            .map_or_default(|data| data.groups.iter().map(|group| group.root.clone()).collect());
        let reply = self
            .environments
            .execute(command, request, &mut self.outbox)?;
        if let (Some((environment, _)), Reply::Accepted) = (&closed, &reply) {
            self.chats
                .close_environment(environment.as_str(), &mut self.outbox);
            self.launches.forget_environment(environment);
        }
        if let (Some((environment, host)), Reply::Accepted) = (closed, &reply) {
            self.chats.mentions.drop_roots(&host, &roots);
            let still_used = self.environments.uses_host(&host);
            self.hosts.environment_closed(&environment, still_used);
        }
        Some(reply)
    }

    pub(super) fn detect_host_agents(&self, environment: &EnvironmentName) {
        let acp_agents = self.settings.current.acp.all_agents();
        self.hosts
            .detect_agents(environment, acp_agents, &self.jobs);
    }

    pub(super) fn tick_hosts(&mut self) {
        let (hosts, mut work) = self.host_work();
        hosts.tick(Instant::now(), &mut work);
    }

    pub(super) fn launch_helper(&mut self, terminal: AgentRef) -> Arc<Shared> {
        self.outbox
            .push(EventKind::TerminalLaunched(TerminalLaunched {
                terminal: terminal.clone(),
                harness: None,
                launched_as: LaunchedAs::Line,
                resume: None,
            }));
        self.launches.view(terminal)
    }

    fn open_watched_sign_in(&mut self, terminal: AgentRef) {
        let shared = self.launch_helper(terminal.clone());
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
            } => {
                self.hosts
                    .bus_prepared(&environment, &host, prepared, &mut self.outbox);
                self.flush_chats();
            }
            JobDone::Answered { request, answer } => {
                self.outbox.push(EventKind::Answered { request, answer });
            }
            JobDone::Buffer(done) => self.buffer_job_done(done),
            JobDone::Language(done) => self.language_job_done(done),
            JobDone::FolderListed { folder, listed } => self.folder_listed((folder, listed)),
            JobDone::Uploaded { request, answer } => {
                self.uploads.finished(request, &mut self.outbox);
                self.outbox.push(EventKind::Answered { request, answer });
            }
            JobDone::ChatOpened(opening) => self.chat_opened(opening),
            JobDone::ChatEarlier(earlier) => self.chat_earlier_read(earlier),
            JobDone::ChatCheckpoint(taken) => self.chat_checkpoint_taken(taken),
            JobDone::ChatImage(done) => self.chat_image_done(done),
            JobDone::FilesIndexed(indexed) => self.files_indexed(indexed),
            JobDone::GatewayChecked(read) => {
                self.gateway.checked(read, &mut self.outbox);
                self.flush_chats();
            }
            JobDone::GatewayForgotten(forgotten) => {
                Gateway::forgotten(forgotten, &mut self.outbox);
            }
            JobDone::OpencodeStarted(started) => self.opencode_started(started),
            JobDone::OpencodeListed(listed) => self.opencode_listed(listed),
            JobDone::AcpSecretsRead(read) => self.acp_secrets_read(read),
            JobDone::AttachedLine(line) => self.attached_line_ready(line),
            JobDone::OpencodeTerminal { terminal, state } => {
                self.opencode_terminal_state(terminal, state);
            }
            JobDone::SignInEnvRead(read) => self.sign_in_env_read(read),
            JobDone::HostAgentsFound(found) => {
                self.hosts.agents_found(found, &mut self.outbox);
                self.flush_chats();
            }
            JobDone::LocalAgentsFound(found) => {
                self.hosts.local_agents_found(found, &mut self.outbox);
            }
            JobDone::McpRead(read) => self.with_chat_work(|chats, work| chats.mcp_read(read, work)),
            JobDone::McpLaunched(launched) => {
                self.with_chat_work(|chats, work| chats.mcp_launched(launched, work));
            }
            JobDone::HarnessChecked { .. }
            | JobDone::ModelsListed { .. }
            | JobDone::AcpOffersRead(_)
            | JobDone::UsageRead { .. }
            | JobDone::Git(_)
            | JobDone::KraddieLines(_)
            | JobDone::Adapter(_)
            | JobDone::Variant(_)
            | JobDone::CodeHosts(_)
            | JobDone::Review(_)
            | JobDone::Runner(_)
            | JobDone::Cloud(_)
            | JobDone::CloudLiveness(_)
            | JobDone::CloudOpencodeStarted(_) => {}
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
