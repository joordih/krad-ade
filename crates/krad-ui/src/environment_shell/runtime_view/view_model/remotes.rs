use gpui_kit::{AppContext as _, Context};
use krad_client::{ClientError, Cursor, Runner, RuntimeClient};
use krad_domain::{CloudRunnerId, EnvironmentName, RunnerState, RuntimeId, Session};
use krad_protocol::{AgentRef, Feed, HostedSummary};
use krad_runtime::{LineStop, RunnerLineError, RuntimeHandle};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use super::super::following::{Batch, Following};
use super::{Area, Heard, RuntimeNews, ViewModel};

mod backoff;
mod remote;

pub(in crate::environment_shell) use remote::{
    Folded, HostedFailure, RemoteChat, RemoteRuntime, RunnerReach,
};

pub(in crate::environment_shell) enum Reached {
    Connected {
        client: Arc<dyn RuntimeClient>,
        build: String,
        stop: Option<LineStop>,
    },
    ProcessWire,
    Unreachable,
}

#[derive(Default)]
pub(in crate::environment_shell) struct Remotes {
    runtimes: BTreeMap<CloudRunnerId, RemoteRuntime>,
    connections: u64,
    awaited: BTreeMap<String, CloudRunnerId>,
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
        self.runtimes.insert(runner, RemoteRuntime::new(connection));
        connection
    }

    fn reaching_again(&mut self, runner: CloudRunnerId, connection: u64) -> Option<u64> {
        self.connections += 1;
        let next = self.connections;
        let remote = self.current(runner, connection)?;
        remote.connection = next;
        Some(next)
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

    pub(in crate::environment_shell) fn hosted(
        &self,
    ) -> impl Iterator<Item = (CloudRunnerId, &HostedSummary)> {
        self.runtimes.iter().flat_map(|(runner, remote)| {
            remote.hosted.iter().map(move |summary| (*runner, summary))
        })
    }

    pub(in crate::environment_shell) fn chat(
        &self,
        runner: CloudRunnerId,
        chat: &AgentRef,
    ) -> Option<&RemoteChat> {
        self.runtimes.get(&runner)?.chats.get(chat)
    }

    pub(in crate::environment_shell) fn route(
        &self,
        local: &Arc<dyn RuntimeClient>,
        runtime: RuntimeId,
    ) -> Option<Arc<dyn RuntimeClient>> {
        match runtime {
            RuntimeId::Local => Some(Arc::clone(local)),
            RuntimeId::Runner(runner) => self.runtimes.get(&runner)?.client.clone(),
        }
    }

    pub(in crate::environment_shell) fn connected(&self, runner: CloudRunnerId) -> bool {
        self.runtimes
            .get(&runner)
            .is_some_and(|remote| matches!(remote.reach, RunnerReach::Connected { .. }))
    }

    pub(in crate::environment_shell) fn await_hosted(
        &mut self,
        runner: CloudRunnerId,
        slug: String,
    ) {
        self.awaited.insert(slug, runner);
    }

    fn awaited_failure(
        &mut self,
        runner: CloudRunnerId,
        failure: HostedFailure,
    ) -> Option<HostedFailure> {
        (self.awaited.get(&failure.slug) == Some(&runner)).then(|| {
            self.awaited.remove(&failure.slug);
            failure
        })
    }

    fn hosted_arrived(&mut self, runner: CloudRunnerId) {
        let Some(remote) = self.runtimes.get(&runner) else {
            return;
        };
        let branches: Vec<&str> = remote
            .hosted
            .iter()
            .map(|summary| summary.branch.as_str())
            .collect();
        self.awaited.retain(|slug, waits| {
            *waits != runner
                || !branches
                    .iter()
                    .any(|branch| branch.ends_with(slug.as_str()))
        });
    }
}

fn reached(connected: Result<Runner, ClientError>, stop: LineStop) -> Reached {
    match connected {
        Ok(runner) => {
            let build = runner.welcome().build.clone();
            Reached::Connected {
                client: Arc::new(runner),
                build,
                stop: Some(stop),
            }
        }
        Err(error) => {
            tracing::debug!(%error, "the runner's runtime did not welcome this window");
            stop.stop();
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
    reached(
        Runner::connect(line.input, line.output, line.hello),
        line.stop,
    )
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
            Self::reach(handle.clone(), runner, connection, cx);
        }
    }

    fn reach(
        handle: RuntimeHandle,
        runner: CloudRunnerId,
        connection: u64,
        cx: &mut Context<Self>,
    ) {
        let reaching = cx.background_spawn(connect(handle.clone(), runner));
        cx.spawn(async move |this, cx| {
            let reached = reaching.await;
            this.update(cx, |model, cx| {
                model.remote_reached((runner, connection), reached, handle, cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn remote_reached(
        &mut self,
        (runner, connection): (CloudRunnerId, u64),
        reached: Reached,
        handle: RuntimeHandle,
        cx: &mut Context<Self>,
    ) {
        if let Some((client, cursor)) = self.adopt_reached(runner, connection, reached) {
            Self::follow_remote((runner, connection), client, cursor, handle, cx);
        }
        cx.emit(RuntimeNews);
    }

    pub(super) fn adopt_reached(
        &mut self,
        runner: CloudRunnerId,
        connection: u64,
        reached: Reached,
    ) -> Option<(Arc<dyn RuntimeClient>, Cursor)> {
        let remote = self.remotes.current(runner, connection)?;
        let follow = match reached {
            Reached::Connected {
                client,
                build,
                stop,
            } => {
                remote.let_go();
                remote.reach = RunnerReach::Connected { build };
                remote.client = Some(Arc::clone(&client));
                remote.stop = stop;
                Some((client, remote.cursor))
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
        (runner, connection): (CloudRunnerId, u64),
        client: Arc<dyn RuntimeClient>,
        cursor: Cursor,
        handle: RuntimeHandle,
        cx: &mut Context<Self>,
    ) {
        let mut following = Following::resuming(client, cursor);
        cx.spawn(async move |this, cx| loop {
            let Batch::Feeds(feeds) = following.next_batch().await else {
                let cursor = following.cursor();
                let wait = this.update(cx, |model, cx| {
                    model.remote_lost((runner, connection), cursor, cx)
                });
                if let Ok(Some(wait)) = wait {
                    cx.background_executor().timer(wait).await;
                    this.update(cx, |model, cx| {
                        model.reach_again((runner, connection), handle, cx);
                    })
                    .unwrap_or_default();
                }
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
        if let Some(remote) = self.remotes.current(runner, connection) {
            remote.backoff.reset();
            if changed {
                remote.refresh_hosted();
                self.remotes.hosted_arrived(runner);
            }
        }
        if changed || !self.news.heard.is_empty() {
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
        let Folded { changed, failure } = match feed {
            Feed::Snapshot(snapshot) => {
                remote.adopt(snapshot);
                Folded {
                    changed: true,
                    failure: None,
                }
            }
            Feed::Event(event) => remote.fold(event),
        };
        if let Some(failure) =
            failure.and_then(|failure| self.remotes.awaited_failure(runner, failure))
        {
            self.heard(Heard::HostedFailed { runner, failure });
        }
        Some(changed)
    }

    fn remote_lost(
        &mut self,
        (runner, connection): (CloudRunnerId, u64),
        cursor: Cursor,
        cx: &mut Context<Self>,
    ) -> Option<Duration> {
        let ready = self.runners.get(&runner).is_some_and(RunnerState::is_ready);
        let remote = self.remotes.current(runner, connection)?;
        remote.let_go();
        remote.cursor = cursor;
        remote.reach = RunnerReach::Connecting;
        let wait = remote.backoff.next_wait();
        if !ready {
            self.remotes.retain(|kept| kept != runner);
        }
        self.changed(Area::Runners);
        cx.emit(RuntimeNews);
        ready.then_some(wait)
    }

    fn reach_again(
        &mut self,
        (runner, connection): (CloudRunnerId, u64),
        handle: RuntimeHandle,
        cx: &mut Context<Self>,
    ) {
        if !self.runners.get(&runner).is_some_and(RunnerState::is_ready) {
            return;
        }
        let Some(next) = self.remotes.reaching_again(runner, connection) else {
            return;
        };
        Self::reach(handle, runner, next, cx);
    }

    pub(super) fn forget_unready_remotes(&mut self) {
        let runners = &self.runners;
        self.remotes
            .retain(|runner| runners.get(&runner).is_some_and(RunnerState::is_ready));
    }
}

#[cfg(test)]
mod tests;
