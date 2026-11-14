use gpui_kit::{AppContext as _, Context};
use krad_client::{ClientError, Runner, RuntimeClient};
use krad_domain::{CloudRunnerId, EnvironmentName, RunnerState, RuntimeId, Session};
use krad_protocol::{AskId, EventKind, Feed, Snapshot};
use krad_runtime::{RunnerLineError, RuntimeHandle};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use super::super::following::{Batch, Following};
use super::super::EnvironmentsMirror;
use super::{Area, RuntimeNews, ViewModel};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) enum RunnerReach {
    Connecting,
    Connected { build: String },
    ProcessWire,
}

pub(in crate::environment_shell) enum Reached {
    Connected(Arc<dyn RuntimeClient>, String),
    ProcessWire,
    Unreachable,
}

pub(in crate::environment_shell) struct RemoteRuntime {
    pub(in crate::environment_shell) reach: RunnerReach,
    pub(in crate::environment_shell) environments: EnvironmentsMirror,
    pub(in crate::environment_shell) asks: BTreeSet<AskId>,
    connection: u64,
}

#[derive(Default)]
pub(in crate::environment_shell) struct Remotes {
    runtimes: BTreeMap<CloudRunnerId, RemoteRuntime>,
    connections: u64,
}

impl Remotes {
    pub(in crate::environment_shell) fn get(
        &self,
        runner: CloudRunnerId,
    ) -> Option<&RemoteRuntime> {
        self.runtimes.get(&runner)
    }

    fn current(&mut self, runner: CloudRunnerId, connection: u64) -> Option<&mut RemoteRuntime> {
        self.runtimes
            .get_mut(&runner)
            .filter(|remote| remote.connection == connection)
    }

    pub(super) fn reaching(&mut self, runner: CloudRunnerId) -> u64 {
        self.connections += 1;
        let connection = self.connections;
        self.runtimes.insert(
            runner,
            RemoteRuntime {
                reach: RunnerReach::Connecting,
                environments: EnvironmentsMirror::default(),
                asks: BTreeSet::new(),
                connection,
            },
        );
        connection
    }

    pub(in crate::environment_shell) fn retain(&mut self, keep: impl Fn(CloudRunnerId) -> bool) {
        self.runtimes.retain(|runner, _| keep(*runner));
    }

    pub(in crate::environment_shell) fn sessions(
        &self,
    ) -> impl Iterator<Item = (RuntimeId, &EnvironmentName, &Session)> {
        self.runtimes.iter().flat_map(|(runner, remote)| {
            remote
                .environments
                .all()
                .iter()
                .flat_map(move |environment| {
                    environment.sessions.iter().map(move |session| {
                        (RuntimeId::Runner(*runner), &environment.name, session)
                    })
                })
        })
    }
}

impl RemoteRuntime {
    fn adopt(&mut self, snapshot: Snapshot) {
        self.environments.replace(snapshot.environments);
        self.asks = snapshot
            .bus
            .asks
            .iter()
            .map(|pending| pending.ask)
            .collect();
    }

    fn fold(&mut self, kind: EventKind) -> bool {
        match kind {
            EventKind::EnvironmentOpened { environment } => self.environments.opened(*environment),
            EventKind::EnvironmentClosed { environment } => self.environments.closed(&environment),
            EventKind::EnvironmentsOrdered { order } => self.environments.ordered(&order),
            EventKind::EnvironmentChanged {
                environment,
                changes,
                ..
            } => self.environments.changed(&environment, changes),
            EventKind::AskPending(pending) => return self.asks.insert(pending.ask),
            EventKind::AskSettled { ask, .. } => return self.asks.remove(&ask),
            _ => return false,
        }
        true
    }
}

fn reached(connected: Result<Runner, ClientError>) -> Reached {
    match connected {
        Ok(runner) => {
            let build = runner.welcome().build.clone();
            Reached::Connected(Arc::new(runner), build)
        }
        Err(error) => {
            tracing::debug!(%error, "the runner's runtime did not welcome this window");
            Reached::ProcessWire
        }
    }
}

async fn connect(handle: RuntimeHandle, runner: CloudRunnerId) -> Reached {
    let line = match handle.connect_runner(runner).await {
        Ok(Ok(line)) => line,
        Ok(Err(RunnerLineError::NotReady)) | Err(_) => return Reached::Unreachable,
        Ok(Err(error @ RunnerLineError::Failed(_))) => {
            tracing::debug!(%error, "the runner's runtime is out of reach");
            return Reached::ProcessWire;
        }
    };
    reached(Runner::connect(line.input, line.output, line.hello))
}

impl ViewModel {
    pub(in crate::environment_shell) fn reach_ready_runners(
        &mut self,
        handle: &RuntimeHandle,
        cx: &mut Context<Self>,
    ) {
        let wanted: Vec<CloudRunnerId> = self
            .runners
            .iter()
            .filter(|(runner, state)| state.is_ready() && self.remotes.get(**runner).is_none())
            .map(|(runner, _)| *runner)
            .collect();
        for runner in wanted {
            let connection = self.remotes.reaching(runner);
            let reaching = cx.background_spawn(connect(handle.clone(), runner));
            cx.spawn(async move |this, cx| {
                let reached = reaching.await;
                this.update(cx, |model, cx| {
                    model.remote_reached(runner, connection, reached, cx);
                })
                .unwrap_or_default();
            })
            .detach();
        }
    }

    fn remote_reached(
        &mut self,
        runner: CloudRunnerId,
        connection: u64,
        reached: Reached,
        cx: &mut Context<Self>,
    ) {
        if let Some(client) = self.adopt_reached(runner, connection, reached) {
            Self::follow_remote(runner, connection, client, cx);
        }
        cx.emit(RuntimeNews);
    }

    pub(super) fn adopt_reached(
        &mut self,
        runner: CloudRunnerId,
        connection: u64,
        reached: Reached,
    ) -> Option<Arc<dyn RuntimeClient>> {
        let remote = self.remotes.current(runner, connection)?;
        let follow = match reached {
            Reached::Connected(client, build) => {
                remote.reach = RunnerReach::Connected { build };
                Some(client)
            }
            Reached::ProcessWire => {
                remote.reach = RunnerReach::ProcessWire;
                None
            }
            Reached::Unreachable => {
                self.remotes.retain(|kept| kept != runner);
                return None;
            }
        };
        self.changed(Area::Runners);
        follow
    }

    fn follow_remote(
        runner: CloudRunnerId,
        connection: u64,
        client: Arc<dyn RuntimeClient>,
        cx: &mut Context<Self>,
    ) {
        let mut following = Following::new(client);
        cx.spawn(async move |this, cx| loop {
            let Batch::Feeds(feeds) = following.next_batch().await else {
                this.update(cx, |model, cx| model.remote_lost(runner, connection, cx))
                    .unwrap_or_default();
                break;
            };
            let heard = this.update(cx, |model, cx| {
                model.hear_remote(runner, connection, feeds, cx)
            });
            let Ok(Some(pace)) = heard else {
                break;
            };
            cx.background_executor().timer(pace).await;
        })
        .detach();
    }

    fn hear_remote(
        &mut self,
        runner: CloudRunnerId,
        connection: u64,
        feeds: Vec<Feed>,
        cx: &mut Context<Self>,
    ) -> Option<Duration> {
        let mut changed = false;
        for feed in feeds {
            changed |= self.fold_remote(runner, connection, feed)?;
        }
        if changed {
            self.changed(Area::Runners);
            cx.emit(RuntimeNews);
        }
        Some(self.pace)
    }

    pub(in crate::environment_shell) fn fold_remote(
        &mut self,
        runner: CloudRunnerId,
        connection: u64,
        feed: Feed,
    ) -> Option<bool> {
        let remote = self.remotes.current(runner, connection)?;
        Some(match feed {
            Feed::Snapshot(snapshot) => {
                remote.adopt(snapshot);
                true
            }
            Feed::Event(event) => remote.fold(event.kind),
        })
    }

    fn remote_lost(&mut self, runner: CloudRunnerId, connection: u64, cx: &mut Context<Self>) {
        if self.remotes.current(runner, connection).is_none() {
            return;
        }
        self.remotes.retain(|kept| kept != runner);
        self.changed(Area::Runners);
        cx.emit(RuntimeNews);
    }

    pub(super) fn forget_unready_remotes(&mut self) {
        let runners = &self.runners;
        self.remotes
            .retain(|runner| runners.get(&runner).is_some_and(RunnerState::is_ready));
    }
}
