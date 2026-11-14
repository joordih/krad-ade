use krad_domain::{
    ClaudeOauthToken, CloudRunnerId, EnvironmentName, Host, HostPlatform, Linger, Pairing,
    RunnerCli, RunnerLogin, RunnerState,
};
use krad_host::{Forward, HostError, HostLink};
use krad_protocol::{AgentRef, RunnerLogins, RunnerShown};
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

mod bus;
mod explain;
mod install;
mod line;
mod logins;
mod own;
mod sign_in;
mod webhooks;

pub(crate) use bus::RunnerBuses;
#[cfg(test)]
pub(crate) use webhooks::webhook_target;
pub(crate) use webhooks::{runner_webhooks_plan, RunnerWebhooksSync};

pub(crate) use explain::explain;
pub(crate) use install::{prepare_runner, retire_runner, PreparedRunner, RunnerSetupError};
pub(crate) use line::open_runner_line;
pub use line::{LineStop, RunnerLine, RunnerLineError};
pub(crate) use logins::{git_program, login_args, probe_login, Logins};
pub(crate) use own::OwnActivity;
pub use own::{OwnRunner, OWN_RUNNER};
pub(crate) use sign_in::{forward_failure, login_script, LoopbackCallback, SignInWatch, SignIns};

#[derive(Clone)]
pub(crate) struct ReadyRunner {
    pub(crate) link: HostLink,
    pub(crate) cli: PathBuf,
    pub(crate) pairing: Pairing,
    pub(crate) claude_token: Option<ClaudeOauthToken>,
}

impl fmt::Debug for ReadyRunner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReadyRunner")
            .field("cli", &self.cli)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Default)]
pub(crate) struct RunnerDirectory(HashMap<CloudRunnerId, ReadyRunner>);

impl RunnerDirectory {
    pub(crate) fn get(&self, runner: CloudRunnerId) -> Option<ReadyRunner> {
        self.0.get(&runner).cloned()
    }

    fn put(&mut self, runner: CloudRunnerId, ready: ReadyRunner) {
        self.0.insert(runner, ready);
    }

    pub(crate) fn keep_claude_token(
        &mut self,
        runner: CloudRunnerId,
        token: Option<ClaudeOauthToken>,
    ) {
        if let Some(ready) = self.0.get_mut(&runner) {
            ready.claude_token = token;
        }
    }

    fn take(&mut self, runner: CloudRunnerId) -> Option<ReadyRunner> {
        self.0.remove(&runner)
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
    LoginProbed {
        runner: CloudRunnerId,
        cli: RunnerCli,
        login: RunnerLogin,
    },
    ClaudeTokenSaved {
        runner: CloudRunnerId,
        token: Option<ClaudeOauthToken>,
        saved: Result<(), String>,
    },
    SignInForwarded {
        terminal: AgentRef,
        callback: LoopbackCallback,
        opened: Result<Forward, HostError>,
    },
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

#[derive(Debug, Default)]
pub(crate) struct Runners {
    live: HashMap<CloudRunnerId, LiveRunner>,
    pub(crate) directory: RunnerDirectory,
    pub(crate) buses: RunnerBuses,
    pub(crate) webhooks: RunnerWebhooksSync,
    pub(crate) logins: Logins,
    pub(crate) sign_ins: SignIns,
    pub(crate) own: Option<EnvironmentName>,
    pub(crate) activity: OwnActivity,
}

impl Runners {
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

    pub(crate) fn adopt_own(&mut self, own: OwnRunner) {
        self.track(OWN_RUNNER, Host::Local);
        let ready = ReadyRunner {
            link: HostLink::Local,
            cli: own.cli,
            pairing: own.pairing,
            claude_token: None,
        };
        let state = RunnerState::Ready {
            linger: Linger::Enabled,
        };
        if self.ready(OWN_RUNNER, ready, state).is_some() {
            self.own = Some(own.environment);
        }
    }

    pub(crate) fn forget(&mut self, runner: CloudRunnerId) -> Option<(Host, Option<ReadyRunner>)> {
        let live = self.live.remove(&runner)?;
        Some((live.host, self.directory.take(runner)))
    }

    pub(crate) fn logins_shown(&self, runner: CloudRunnerId) -> RunnerLogins {
        let token = self
            .directory
            .get(runner)
            .is_some_and(|ready| ready.claude_token.is_some());
        self.logins.shown(runner, token)
    }

    pub(crate) fn logins_snapshot(&self) -> Vec<RunnerLogins> {
        let mut runners: Vec<CloudRunnerId> = self.logins.runners().collect();
        runners.extend(self.live.keys().copied());
        runners.sort_unstable();
        runners.dedup();
        runners
            .into_iter()
            .map(|runner| self.logins_shown(runner))
            .collect()
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
