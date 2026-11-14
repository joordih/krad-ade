use gpui_kit::{Context, EventEmitter};
use krad_client::RuntimeClient;
use krad_domain::{
    AcpAgentId, AgentPreset, AgentStatus, CloudRunnerId, EnvironmentName, ManagedAdapter,
    RunnerState, Settings,
};
use krad_protocol::{AskId, EventKind, Feed, PendingAsk, RemoteBusState, RunnerLogins, Snapshot};
use std::collections::{BTreeSet, HashMap};
use std::mem::take;
use std::sync::Arc;
use std::time::Duration;

use super::super::attention::AttentionMirror;
use super::super::harness_health::HealthMirror;
use super::super::harness_models::{ListedModels, ModelsMirror};
use super::super::opencode::OpencodeMirror;
use super::super::usage_bar::UsageMirror;
use super::following::{Batch, Following};
use super::settings::SettingsMirror;
use super::{AgentSpot, HostMove, HostsMirror, MailNotices, Waiters};

mod folding;
mod remotes;

pub(in crate::environment_shell) use remotes::{HostedFailure, Remotes, RunnerReach};

const FIRST_PACE: Duration = Duration::from_millis(16);
const RECONNECT_PAUSE: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(in crate::environment_shell) enum Area {
    Statuses,
    Mail,
    Attention,
    Health,
    Usage,
    Models,
    Opencode,
    Hosts,
    Settings,
    Runners,
}

impl Area {
    const ALL: [Self; 10] = [
        Self::Statuses,
        Self::Mail,
        Self::Attention,
        Self::Health,
        Self::Usage,
        Self::Models,
        Self::Opencode,
        Self::Hosts,
        Self::Settings,
        Self::Runners,
    ];
}

pub(in crate::environment_shell) enum Heard {
    Snapshot {
        snapshot: Box<Snapshot>,
        moves: Vec<HostMove>,
    },
    Event(EventKind),
    HostMoved(HostMove),
    RemoteBusMoved {
        environment: EnvironmentName,
        before: RemoteBusState,
    },
    AskHeld(PendingAsk),
    AskLetGo(AskId),
    ModelsRelisted(AgentPreset),
    OpencodeReady,
    SettingsAdopted(Box<Settings>),
    HostedFailed {
        runner: CloudRunnerId,
        failure: HostedFailure,
    },
    RemoteChat {
        runner: CloudRunnerId,
        kind: Box<EventKind>,
    },
    RemoteReplaced(CloudRunnerId),
}

#[derive(Default)]
pub(in crate::environment_shell) struct News {
    pub(in crate::environment_shell) heard: Vec<Heard>,
    pub(in crate::environment_shell) areas: BTreeSet<Area>,
}

pub(in crate::environment_shell) struct RuntimeNews;

pub(in crate::environment_shell) struct ViewModel {
    pub(in crate::environment_shell) statuses: HashMap<AgentSpot, AgentStatus>,
    pub(in crate::environment_shell) asks: BTreeSet<AskId>,
    pub(in crate::environment_shell) mail: MailNotices,
    pub(in crate::environment_shell) attention: AttentionMirror,
    pub(in crate::environment_shell) health: HealthMirror,
    pub(in crate::environment_shell) usage: UsageMirror,
    pub(in crate::environment_shell) models: ModelsMirror,
    pub(in crate::environment_shell) acp: ListedModels,
    pub(in crate::environment_shell) adapters: HashMap<AcpAgentId, ManagedAdapter>,
    pub(in crate::environment_shell) opencode: OpencodeMirror,
    pub(in crate::environment_shell) hosts: HostsMirror,
    pub(in crate::environment_shell) settings: SettingsMirror,
    pub(in crate::environment_shell) runners: HashMap<CloudRunnerId, RunnerState>,
    pub(in crate::environment_shell) runner_buses: HashMap<CloudRunnerId, RemoteBusState>,
    pub(in crate::environment_shell) runner_logins: HashMap<CloudRunnerId, RunnerLogins>,
    pub(in crate::environment_shell) remotes: Remotes,
    pub(in crate::environment_shell) pace: Duration,
    pub(in crate::environment_shell::runtime_view) waiters: Waiters,
    news: News,
}

impl Default for ViewModel {
    fn default() -> Self {
        Self {
            statuses: HashMap::new(),
            asks: BTreeSet::new(),
            mail: MailNotices::default(),
            attention: AttentionMirror::default(),
            health: HealthMirror::default(),
            usage: UsageMirror::default(),
            models: ModelsMirror::default(),
            adapters: HashMap::new(),
            acp: ListedModels::default(),
            opencode: OpencodeMirror::default(),
            hosts: HostsMirror::default(),
            settings: SettingsMirror::default(),
            runners: HashMap::new(),
            runner_buses: HashMap::new(),
            runner_logins: HashMap::new(),
            remotes: Remotes::default(),
            pace: FIRST_PACE,
            waiters: Waiters::default(),
            news: News::default(),
        }
    }
}

impl EventEmitter<RuntimeNews> for ViewModel {}

impl ViewModel {
    pub(in crate::environment_shell) fn follow(
        client: Arc<dyn RuntimeClient>,
        cx: &mut Context<Self>,
    ) {
        let mut following = Following::new(client);
        cx.spawn(async move |this, cx| loop {
            let pause = match following.next_batch().await {
                Batch::Feeds(feeds) => {
                    let Ok(pace) = this.update(cx, |model, cx| model.hear(feeds, cx)) else {
                        break;
                    };
                    pace
                }
                Batch::Ended => {
                    let Ok(()) = this.update(cx, |model, _| model.waiters.forget_all()) else {
                        break;
                    };
                    RECONNECT_PAUSE
                }
                Batch::Stopped => break,
            };
            cx.background_executor().timer(pause).await;
        })
        .detach();
    }

    fn hear(&mut self, feeds: Vec<Feed>, cx: &mut Context<Self>) -> Duration {
        for feed in feeds {
            self.feed(feed);
        }
        if !self.news.heard.is_empty() || !self.news.areas.is_empty() {
            cx.emit(RuntimeNews);
        }
        self.pace
    }

    pub(in crate::environment_shell) fn feed(&mut self, feed: Feed) {
        match feed {
            Feed::Snapshot(snapshot) => self.adopt_snapshot(snapshot),
            Feed::Event(event) => self.fold_event(event.kind),
        }
    }

    pub(in crate::environment_shell) fn take_news(&mut self) -> News {
        take(&mut self.news)
    }

    fn changed(&mut self, area: Area) {
        self.news.areas.insert(area);
    }

    fn heard(&mut self, heard: Heard) {
        self.news.heard.push(heard);
    }
}

#[cfg(test)]
mod tests;
