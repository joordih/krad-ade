use krad_chat::stop_cloud_process;
use krad_domain::{CloudRunnerId, Host, HostPlatform, RunnerState};
use krad_host::{HostError, HostLink};
use krad_protocol::{ErrorCode, EventKind, Reply, RunnerCommand, RunnerRemoved, RunnerShown};
use krad_store::{
    delete_cloud_claude_token, delete_cloud_opencode_password, delete_cloud_runner_token,
};

use super::{refused, Runtime, UNSUPPORTED_COMMAND};
use crate::jobs::JobDone;
use crate::runners::{
    explain, prepare_runner, retire_runner, PreparedRunner, ReadyRunner, RunnerDone,
    RunnerSetupError,
};

const UNKNOWN_RUNNER: &str = "no cloud runner with this id in the settings";
const NOT_STARTED: &str = "could not start reaching the cloud runner";
const RETIRE_FAILED: &str = "The cloud runner was removed, but cleaning up its host failed";
const TOKEN_NOT_DELETED: &str = "Could not forget the cloud runner's pairing token";

impl From<RunnerDone> for JobDone {
    fn from(done: RunnerDone) -> Self {
        Self::Runner(Box::new(done))
    }
}

impl Runtime {
    pub(super) fn execute_runner(&mut self, command: RunnerCommand) -> Reply {
        match command {
            RunnerCommand::Ensure { runner } => self.ensure_runner(runner),
            RunnerCommand::Reconnect { runner } => {
                if !self.track_runner(runner) {
                    return refused(ErrorCode::NotFound, UNKNOWN_RUNNER);
                }
                if !self.runners.state(runner).is_busy() {
                    self.connect_runner(runner);
                }
                Reply::Accepted
            }
            RunnerCommand::Remove { runner } => self.remove_runner(runner),
            RunnerCommand::Unknown => refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND),
        }
    }

    fn track_runner(&mut self, runner: CloudRunnerId) -> bool {
        let Some(found) = self.settings.current.cloud.find(runner) else {
            return false;
        };
        let host = Host::Ssh {
            destination: found.destination.clone(),
        };
        self.runners.track(runner, host);
        true
    }

    fn ensure_runner(&mut self, runner: CloudRunnerId) -> Reply {
        if !self.track_runner(runner) {
            return refused(ErrorCode::NotFound, UNKNOWN_RUNNER);
        }
        let state = self.runners.state(runner);
        let link_lost = state.is_ready()
            && self
                .runners
                .directory
                .get(runner)
                .is_none_or(|ready| !ready.link.is_ready());
        if matches!(
            state,
            RunnerState::NotConnected | RunnerState::Failed { .. }
        ) || link_lost
        {
            self.connect_runner(runner);
        }
        Reply::Accepted
    }

    fn show_runner(&mut self, shown: Option<RunnerShown>) {
        if let Some(shown) = shown {
            self.outbox.push(EventKind::RunnerChanged(shown));
        }
    }

    fn runner_link(&self, runner: CloudRunnerId) -> Option<HostLink> {
        self.runners.host(runner).map(|host| self.hosts.link(host))
    }

    #[tracing::instrument(skip_all, fields(%runner))]
    fn connect_runner(&mut self, runner: CloudRunnerId) {
        let Some(HostLink::Remote(ssh)) = self.runner_link(runner) else {
            return;
        };
        let shown = self.runners.set(runner, RunnerState::Connecting);
        self.show_runner(shown);
        let started = self.jobs.run(move || {
            let outcome = ssh.connect();
            RunnerDone::Connected { runner, outcome }.into()
        });
        if let Err(error) = started {
            tracing::warn!(%error, "a cloud runner connection could not start");
            let reason = NOT_STARTED.to_owned();
            let shown = self.runners.set(runner, RunnerState::Failed { reason });
            self.show_runner(shown);
        }
    }

    pub(super) fn runner_done(&mut self, done: RunnerDone) {
        match done {
            RunnerDone::Connected { runner, outcome } => self.runner_connected(runner, outcome),
            RunnerDone::Prepared { runner, prepared } => self.runner_prepared(runner, prepared),
            RunnerDone::Removed {
                runner,
                host,
                problem,
            } => {
                if let Some(host) = host {
                    self.hosts.close(&host);
                }
                self.outbox
                    .push(EventKind::RunnerRemoved(RunnerRemoved { runner, problem }));
            }
        }
    }

    fn runner_connected(
        &mut self,
        runner: CloudRunnerId,
        outcome: Result<HostPlatform, HostError>,
    ) {
        if self.runners.state(runner) != RunnerState::Connecting {
            return;
        }
        if let Err(error) = outcome {
            tracing::info!(%runner, %error, "cloud runner connection failed");
            let reason = match self.runners.host(runner) {
                Some(Host::Ssh { destination }) => explain(&error, destination),
                Some(_) | None => error.to_string(),
            };
            let shown = self.runners.set(runner, RunnerState::Failed { reason });
            return self.show_runner(shown);
        }
        let Some(link) = self.runner_link(runner) else {
            return;
        };
        let HostLink::Remote(ssh) = link.clone() else {
            return;
        };
        let shown = self.runners.set(runner, RunnerState::Installing);
        self.show_runner(shown);
        let install_id = self.hosts.install_id();
        let started = self.jobs.run(move || {
            let prepared = prepare_runner(&link, &ssh, runner, install_id);
            RunnerDone::Prepared { runner, prepared }.into()
        });
        if let Err(error) = started {
            tracing::warn!(%error, "a cloud runner install could not start");
            let reason = NOT_STARTED.to_owned();
            let shown = self.runners.set(runner, RunnerState::Failed { reason });
            self.show_runner(shown);
        }
    }

    fn runner_prepared(
        &mut self,
        runner: CloudRunnerId,
        prepared: Result<PreparedRunner, RunnerSetupError>,
    ) {
        if self.runners.state(runner) != RunnerState::Installing {
            return;
        }
        let ready = prepared.is_ok();
        let shown = match (prepared, self.runner_link(runner)) {
            (Ok(prepared), Some(link)) => {
                let state = RunnerState::from(prepared.report);
                let ready = ReadyRunner {
                    link,
                    cli: prepared.cli,
                    pairing: prepared.pairing,
                    claude_token: prepared.claude_token,
                };
                self.runners.ready(runner, ready, state)
            }
            (Ok(_), None) => None,
            (Err(error), _) => {
                tracing::warn!(%runner, %error, "could not set up the cloud runner");
                let reason = error.to_string();
                self.runners.set(runner, RunnerState::Failed { reason })
            }
        };
        self.show_runner(shown);
        if ready {
            self.run_queued_stops(runner);
        }
    }

    pub(super) fn run_ready_stops(&mut self) {
        let ready: Vec<CloudRunnerId> = self
            .runners
            .snapshot()
            .into_iter()
            .filter(|shown| shown.state.is_ready())
            .map(|shown| shown.runner)
            .collect();
        for runner in ready {
            self.run_queued_stops(runner);
        }
    }

    #[tracing::instrument(skip_all, fields(%runner))]
    fn run_queued_stops(&mut self, runner: CloudRunnerId) {
        let Some(ready) = self.runners.directory.get(runner) else {
            return;
        };
        let stops = self.settings.take_cloud_stops(runner, &mut self.outbox);
        if stops.is_empty() {
            return;
        }
        let started = self.jobs.run_reporting(move |_| {
            for process in stops {
                if let Err(error) =
                    stop_cloud_process(&ready.link, &ready.cli, &ready.pairing, &process)
                {
                    tracing::warn!(%process, %error, "a queued cloud process did not stop");
                }
            }
        });
        if let Err(error) = started {
            tracing::warn!(%error, "the queued cloud stops could not start");
        }
    }

    #[tracing::instrument(skip_all, fields(%runner))]
    fn remove_runner(&mut self, runner: CloudRunnerId) -> Reply {
        let (host, ready) = self
            .runners
            .forget(runner)
            .map_or((None, None), |(host, ready)| (Some(host), ready));
        let shared = host.as_ref().is_some_and(|host| {
            self.environments
                .iter()
                .any(|environment| &environment.host == host)
        });
        let started = self.jobs.run(move || {
            let retired =
                ready
                    .filter(|ready| ready.link.is_ready())
                    .map_or_else(Vec::new, |ready| {
                        let paired = (ready.cli, ready.pairing);
                        retire_runner(&ready.link, Some(&paired))
                    });
            let problem = forget_secrets(runner).or_else(|| {
                retired
                    .first()
                    .map(|error| format!("{RETIRE_FAILED}: {error}"))
            });
            RunnerDone::Removed {
                runner,
                host: host.filter(|_| !shared),
                problem,
            }
            .into()
        });
        match started {
            Ok(()) => Reply::Accepted,
            Err(error) => {
                tracing::warn!(%error, "a cloud runner removal could not start");
                refused(ErrorCode::Unavailable, NOT_STARTED)
            }
        }
    }
}

fn forget_secrets(runner: CloudRunnerId) -> Option<String> {
    delete_cloud_runner_token(runner)
        .and_then(|()| delete_cloud_opencode_password(runner))
        .and_then(|()| delete_cloud_claude_token(runner))
        .err()
        .map(|error| format!("{TOKEN_NOT_DELETED}: {error}"))
}
