use krad_domain::{ClaudeOauthToken, CloudRunnerId, Host, HostPlatform, Pairing, RunnerState};
use krad_host::{HostError, HostLink};
use krad_protocol::RunnerShown;
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, PoisonError, RwLock};

mod bus;
mod explain;
mod install;
mod webhooks;

pub(crate) use bus::RunnerBuses;
#[cfg(test)]
pub(crate) use webhooks::webhook_target;
pub(crate) use webhooks::{runner_webhooks_plan, RunnerWebhooksSync};

pub(crate) use explain::explain;
pub(crate) use install::{prepare_runner, retire_runner, PreparedRunner, RunnerSetupError};

#[derive(Clone)]
pub struct ReadyRunner {
    pub link: HostLink,
    pub cli: PathBuf,
    pub pairing: Pairing,
    pub claude_token: Option<ClaudeOauthToken>,
}

impl fmt::Debug for ReadyRunner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReadyRunner")
            .field("cli", &self.cli)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RunnerDirectory(Arc<RwLock<HashMap<CloudRunnerId, ReadyRunner>>>);

impl RunnerDirectory {
    pub(crate) fn get(&self, runner: CloudRunnerId) -> Option<ReadyRunner> {
        self.0
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&runner)
            .cloned()
    }

    fn put(&self, runner: CloudRunnerId, ready: ReadyRunner) {
        self.0
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(runner, ready);
    }

    fn take(&self, runner: CloudRunnerId) -> Option<ReadyRunner> {
        self.0
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&runner)
    }
}

#[derive(Debug)]
pub(crate) enum RunnerDone {
    Connected {
        runner: CloudRunnerId,
        outcome: Result<HostPlatform, HostError>,
    },
    Prepared {
        runner: CloudRunnerId,
        prepared: Result<PreparedRunner, RunnerSetupError>,
    },
    Removed {
        runner: CloudRunnerId,
        host: Option<Host>,
        problem: Option<String>,
    },
    Returned(Box<RunnerReturn>),
}

#[derive(Debug)]
pub(crate) struct RunnerReturn {
    pub(crate) runner: CloudRunnerId,
    pub(crate) started: std::time::Instant,
    pub(crate) events: Vec<krad_agents::QueuedEvent>,
    pub(crate) bus: Option<Result<crate::hosts::RemoteBus, crate::hosts::HostCliError>>,
}

#[derive(Debug)]
struct LiveRunner {
    host: Host,
    state: RunnerState,
}

#[derive(Debug)]
pub(crate) struct Runners {
    live: HashMap<CloudRunnerId, LiveRunner>,
    pub(crate) directory: RunnerDirectory,
    pub(crate) buses: RunnerBuses,
    pub(crate) webhooks: RunnerWebhooksSync,
}

impl Runners {
    pub(crate) fn with_directory(directory: RunnerDirectory) -> Self {
        Self {
            live: HashMap::new(),
            directory,
            buses: RunnerBuses::default(),
            webhooks: RunnerWebhooksSync::default(),
        }
    }

    pub(crate) fn state(&self, runner: CloudRunnerId) -> RunnerState {
        self.live
            .get(&runner)
            .map_or_else(RunnerState::default, |live| live.state.clone())
    }

    pub(crate) fn host(&self, runner: CloudRunnerId) -> Option<&Host> {
        self.live.get(&runner).map(|live| &live.host)
    }

    pub(crate) fn track(&mut self, runner: CloudRunnerId, host: Host) {
        self.live.entry(runner).or_insert(LiveRunner {
            host,
            state: RunnerState::NotConnected,
        });
    }

    pub(crate) fn set(&mut self, runner: CloudRunnerId, state: RunnerState) -> Option<RunnerShown> {
        let live = self.live.get_mut(&runner)?;
        if !state.is_ready() {
            self.directory.take(runner);
        }
        live.state = state.clone();
        Some(RunnerShown { runner, state })
    }

    pub(crate) fn ready(
        &mut self,
        runner: CloudRunnerId,
        ready: ReadyRunner,
        state: RunnerState,
    ) -> Option<RunnerShown> {
        let shown = self.set(runner, state)?;
        self.directory.put(runner, ready);
        Some(shown)
    }

    pub(crate) fn forget(&mut self, runner: CloudRunnerId) -> Option<(Host, Option<ReadyRunner>)> {
        let live = self.live.remove(&runner)?;
        Some((live.host, self.directory.take(runner)))
    }

    pub(crate) fn snapshot(&self) -> Vec<RunnerShown> {
        let mut shown: Vec<RunnerShown> = self
            .live
            .iter()
            .map(|(runner, live)| RunnerShown {
                runner: *runner,
                state: live.state.clone(),
            })
            .collect();
        shown.sort_by_key(|shown| shown.runner);
        shown
    }
}
