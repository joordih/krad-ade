mod agents;
mod backoff;
mod bus_slot;
mod host_cli;
mod links;
mod problem;
mod prompts;
mod remote_bus;
mod sign_in;
mod state;

pub use host_cli::{bundled_cli, upload_host_cli, HostCliError};
pub use problem::explain_host_problem;
pub use remote_bus::{
    remote_askpass, remote_caller_vars, remote_detached_caller_vars, RemoteBus, RemoteBusPlan,
};

pub(crate) use agents::AgentsFound;
pub(crate) use bus_slot::Reach;
pub(crate) use links::HostLinks;
pub(crate) use state::Attempt;

use krad_domain::{AgentPreset, EnvironmentName, Host, HostPlatform};
use krad_host::{HostError, HostLink, MachineChannel};
use krad_protocol::{
    EnvironmentHost, ErrorCode, EventKind, HostCommand, HostState, HostsSnapshot, Reply,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use self::bus_slot::BusSlot;
use self::prompts::Prompts;
use self::state::{after_failure, AfterFailure, Tracked};
use crate::bus::BusAccess;
use crate::jobs::{JobDone, Jobs};
use crate::runtime::refused;
use crate::terminals::Terminals;

const UNKNOWN_ENVIRONMENT: &str = "this runtime has no environment of that name";
const JOB_FAILED: &str = "Could not start connecting";

#[derive(Debug)]
enum Waiting {
    Nothing,
    SignInTerminal,
    MachineChannel(Attempt),
}

#[derive(Debug)]
struct Entry {
    host: Host,
    tracked: Tracked,
    waiting: Waiting,
    bus: BusSlot,
    bus_port: Option<u16>,
    agents: Vec<AgentPreset>,
}

pub(crate) struct HostWork<'a> {
    pub(crate) jobs: &'a Jobs,
    pub(crate) terminals: &'a mut Terminals,
    pub(crate) outbox: &'a mut Vec<EventKind>,
}

#[derive(Debug)]
pub(crate) struct Hosts {
    links: HostLinks,
    prompts: Prompts,
    bus: Option<Arc<BusAccess>>,
    envs: HashMap<EnvironmentName, Entry>,
}

impl Hosts {
    pub(crate) fn new(bus: Option<Arc<BusAccess>>) -> (Self, HostLinks) {
        let (prompts, sender) = Prompts::open();
        let links = HostLinks::new(sender);
        let hosts = Self {
            links: links.clone(),
            prompts,
            bus,
            envs: HashMap::new(),
        };
        (hosts, links)
    }

    pub(crate) fn execute(
        &mut self,
        command: HostCommand,
        host_of: Option<Host>,
        work: &mut HostWork<'_>,
    ) -> Option<Reply> {
        let (environment, wanted): (EnvironmentName, fn(&HostState) -> bool) = match command {
            HostCommand::Show { environment } => (environment, HostState::connects_when_shown),
            HostCommand::Reconnect { environment } => (environment, HostState::can_reconnect),
            HostCommand::AnswerPrompt { prompt, answer } => {
                return Some(self.prompts.answer(prompt, answer, work.outbox));
            }
            HostCommand::DetectAgents { .. } | HostCommand::Unknown => return None,
        };
        let Some(host) = host_of else {
            return Some(refused(ErrorCode::NotFound, UNKNOWN_ENVIRONMENT));
        };
        if host.is_local() {
            return Some(Reply::Accepted);
        }
        if wanted(&self.entry(&environment, host).tracked.state) {
            self.connect(&environment, Attempt::First, work);
        }
        Some(Reply::Accepted)
    }

    fn entry(&mut self, environment: &EnvironmentName, host: Host) -> &mut Entry {
        let entry = self
            .envs
            .entry(environment.clone())
            .or_insert_with(|| Entry {
                host: host.clone(),
                tracked: Tracked::default(),
                waiting: Waiting::Nothing,
                bus: BusSlot::Absent,
                bus_port: None,
                agents: Vec::new(),
            });
        if entry.host != host {
            *entry = Entry {
                host,
                tracked: Tracked::default(),
                waiting: Waiting::Nothing,
                bus: BusSlot::Absent,
                bus_port: None,
                agents: Vec::new(),
            };
        }
        entry
    }

    fn set(
        &mut self,
        environment: &EnvironmentName,
        tracked: Tracked,
        outbox: &mut Vec<EventKind>,
    ) {
        let Some(entry) = self.envs.get_mut(environment) else {
            return;
        };
        let state = tracked.state.clone();
        entry.tracked = tracked;
        outbox.push(EventKind::HostChanged {
            environment: environment.clone(),
            state,
        });
    }

    #[tracing::instrument(skip_all, fields(environment = environment.as_str(), ?attempt))]
    fn connect(
        &mut self,
        environment: &EnvironmentName,
        attempt: Attempt,
        work: &mut HostWork<'_>,
    ) {
        let Some(host) = self.envs.get(environment).map(|entry| entry.host.clone()) else {
            return;
        };
        self.set(environment, HostState::Connecting.into(), work.outbox);
        let HostLink::Remote(remote) = self.links.link(&host) else {
            return;
        };
        if remote.machine().is_some() {
            if let Some(entry) = self.envs.get_mut(environment) {
                entry.waiting = Waiting::MachineChannel(attempt);
            }
            work.outbox.push(EventKind::MachineChannelWanted {
                environment: environment.clone(),
            });
            return;
        }
        self.run_connect(environment, attempt, work);
    }

    fn run_connect(
        &mut self,
        environment: &EnvironmentName,
        attempt: Attempt,
        work: &mut HostWork<'_>,
    ) {
        let Some(host) = self.envs.get(environment).map(|entry| entry.host.clone()) else {
            return;
        };
        let HostLink::Remote(remote) = self.links.link(&host) else {
            return;
        };
        let job_environment = environment.clone();
        let job_host = host.clone();
        let started = work.jobs.run(move || JobDone::HostConnected {
            environment: job_environment,
            host: job_host,
            attempt,
            outcome: remote.connect(),
        });
        if let Err(error) = started {
            let reason = format!("{JOB_FAILED}: {error}");
            self.set(
                environment,
                HostState::Failed { reason }.into(),
                work.outbox,
            );
        }
    }

    pub(crate) fn machine_channel(
        &mut self,
        environment: &EnvironmentName,
        channel: Result<MachineChannel, String>,
        work: &mut HostWork<'_>,
    ) {
        let Some(entry) = self.envs.get_mut(environment) else {
            return;
        };
        let Waiting::MachineChannel(attempt) =
            std::mem::replace(&mut entry.waiting, Waiting::Nothing)
        else {
            return;
        };
        let host = entry.host.clone();
        let channel = match channel {
            Ok(channel) => channel,
            Err(reason) => {
                return self.set(
                    environment,
                    HostState::Failed { reason }.into(),
                    work.outbox,
                );
            }
        };
        if let HostLink::Remote(remote) = self.links.link(&host) {
            remote.set_machine_channel(channel);
        }
        self.run_connect(environment, attempt, work);
    }

    pub(crate) fn connected(
        &mut self,
        environment: &EnvironmentName,
        (host, attempt): (Host, Attempt),
        outcome: Result<HostPlatform, HostError>,
        work: &mut HostWork<'_>,
    ) {
        let current = self.envs.get(environment).is_some_and(|entry| {
            entry.host == host && entry.tracked.state == HostState::Connecting
        });
        if !current {
            return;
        }
        let error = match outcome {
            Ok(platform) => {
                tracing::info!(environment = environment.as_str(), %platform, "host connected");
                self.prepare_bus(environment, work.jobs, work.outbox);
                self.set(
                    environment,
                    HostState::Connected { platform }.into(),
                    work.outbox,
                );
                return;
            }
            Err(error) => error,
        };
        tracing::info!(environment = environment.as_str(), %error, "host connection failed");
        let explain = || {
            host.destination().map_or_else(
                || error.to_string(),
                |destination| explain_host_problem(&error, destination),
            )
        };
        match after_failure(attempt, &error, Instant::now(), explain) {
            AfterFailure::Next(next) => self.set(environment, next, work.outbox),
            AfterFailure::SignIn => self.want_sign_in(environment, &host, work.outbox),
        }
    }

    pub(crate) fn tick(&mut self, now: Instant, work: &mut HostWork<'_>) {
        self.prompts.collect(work.outbox);
        let due: Vec<(EnvironmentName, u32)> = self
            .envs
            .iter()
            .filter_map(|(name, entry)| Some((name.clone(), entry.tracked.retry_due(now)?)))
            .collect();
        for (environment, attempt) in due {
            self.connect(&environment, Attempt::Retry(attempt), work);
        }
        let dropped: Vec<(EnvironmentName, Tracked)> = self
            .envs
            .iter()
            .filter(|(_, entry)| !self.links.link(&entry.host).is_ready())
            .filter_map(|(name, entry)| Some((name.clone(), entry.tracked.dropped(now)?)))
            .collect();
        for (environment, next) in dropped {
            self.set(&environment, next, work.outbox);
            self.set_bus(&environment, BusSlot::Absent, work.outbox);
        }
        self.drop_lost_buses(work.outbox);
    }

    pub(crate) fn environment_closed(
        &mut self,
        environment: &EnvironmentName,
        host_still_used: bool,
    ) {
        let Some(entry) = self.envs.remove(environment) else {
            return;
        };
        if !host_still_used {
            self.links.close(&entry.host);
        }
    }

    pub(crate) fn snapshot(&self) -> HostsSnapshot {
        HostsSnapshot {
            hosts: self
                .envs
                .iter()
                .map(|(environment, entry)| EnvironmentHost {
                    environment: environment.clone(),
                    state: entry.tracked.state.clone(),
                    bus: entry.bus.shown(),
                    agents: entry.agents.clone(),
                })
                .collect(),
            prompts: self.prompts.snapshot(),
        }
    }
}
