use krad_client::{Cursor, RuntimeClient};
use krad_domain::{AgentPreset, EnvironmentName, ModelOffer};
use krad_protocol::{
    hosted_summaries, AgentRef, AskId, ChatFacts, ChatMirror, Event, EventKind, HostedSummary,
    Problem, ProblemScope, Seq, Snapshot,
};
use krad_runtime::LineStop;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::time::SystemTime;

use super::super::super::super::harness_models::ModelsMirror;
use super::super::super::{EnvironmentsMirror, Waiters};
use super::backoff::Backoff;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) enum RunnerReach {
    Connecting,
    Connected { build: String },
    ProcessWire,
}

pub(in crate::environment_shell) struct RemoteChat {
    pub(in crate::environment_shell) mirror: ChatMirror,
    seq: Option<Seq>,
    heard_at: Option<SystemTime>,
}

impl RemoteChat {
    fn new(mirror: ChatMirror) -> Self {
        Self {
            mirror,
            seq: None,
            heard_at: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct HostedFailure {
    pub(in crate::environment_shell) slug: String,
    pub(in crate::environment_shell) message: String,
}

#[derive(Default)]
pub(in crate::environment_shell) struct Folded {
    pub(in crate::environment_shell) changed: bool,
    pub(in crate::environment_shell) failure: Option<HostedFailure>,
}

pub(in crate::environment_shell) struct RemoteRuntime {
    pub(in crate::environment_shell) reach: RunnerReach,
    pub(in crate::environment_shell) environments: EnvironmentsMirror,
    pub(in crate::environment_shell) asks: BTreeSet<AskId>,
    pub(in crate::environment_shell) chats: HashMap<AgentRef, RemoteChat>,
    pub(in crate::environment_shell) hosted: Vec<HostedSummary>,
    pub(in crate::environment_shell) models: ModelsMirror,
    pub(super) client: Option<Arc<dyn RuntimeClient>>,
    pub(super) stop: Option<LineStop>,
    pub(super) cursor: Cursor,
    pub(super) backoff: Backoff,
    pub(super) connection: u64,
    pub(super) waiters: Waiters,
}

impl Drop for RemoteRuntime {
    fn drop(&mut self) {
        self.let_go();
    }
}

impl RemoteRuntime {
    pub(super) fn new(connection: u64) -> Self {
        Self {
            reach: RunnerReach::Connecting,
            environments: EnvironmentsMirror::default(),
            asks: BTreeSet::new(),
            chats: HashMap::new(),
            hosted: Vec::new(),
            models: ModelsMirror::default(),
            client: None,
            stop: None,
            cursor: Cursor::default(),
            backoff: Backoff::default(),
            connection,
            waiters: Waiters::default(),
        }
    }

    pub(in crate::environment_shell) fn offers(
        &self,
        environment: &EnvironmentName,
        harness: AgentPreset,
    ) -> &[ModelOffer] {
        self.environments
            .all()
            .iter()
            .find(|data| data.name == *environment)
            .map_or(&[], |data| self.models.of(&data.host, harness))
    }

    pub(super) fn let_go(&mut self) {
        self.client = None;
        self.waiters.forget_all();
        if let Some(stop) = self.stop.take() {
            stop.stop();
        }
    }

    pub(super) fn adopt(&mut self, snapshot: Snapshot) {
        self.environments.replace(snapshot.environments);
        self.models.replace(snapshot.models.listed);
        self.asks = snapshot
            .bus
            .asks
            .iter()
            .map(|pending| pending.ask)
            .collect();
        self.chats = snapshot
            .chats
            .into_iter()
            .map(|summary| (summary.tail.chat.clone(), RemoteChat::new(summary.into())))
            .collect();
        for summary in &snapshot.hosted {
            for chat in &summary.chats {
                let id = AgentRef {
                    environment: summary.environment.clone(),
                    terminal: chat.target.terminal,
                };
                if let Some(known) = self.chats.get_mut(&id) {
                    known.seq = chat.seq;
                    known.heard_at = chat.last_activity;
                }
            }
        }
    }

    pub(super) fn fold(&mut self, event: Event) -> Folded {
        let Event { seq, kind } = event;
        let changed = match kind {
            EventKind::EnvironmentOpened { environment } => {
                self.environments.opened(*environment);
                true
            }
            EventKind::EnvironmentClosed { environment } => {
                self.environments.closed(&environment);
                true
            }
            EventKind::EnvironmentsOrdered { order } => {
                self.environments.ordered(&order);
                true
            }
            EventKind::EnvironmentChanged {
                environment,
                changes,
                ..
            } => {
                self.environments.changed(&environment, changes);
                true
            }
            EventKind::Answered { request, answer } => {
                self.waiters.answered(request, *answer);
                false
            }
            EventKind::ModelsListed { listed, .. } => {
                self.models.apply(listed);
                true
            }
            EventKind::AskPending(pending) => self.asks.insert(pending.ask),
            EventKind::AskSettled { ask, .. } => self.asks.remove(&ask),
            EventKind::Problem(Problem {
                scope: ProblemScope::CloudSession { slug },
                message,
                ..
            }) => {
                return Folded {
                    changed: false,
                    failure: Some(HostedFailure { slug, message }),
                };
            }
            kind => self.fold_chat(seq, &kind),
        };
        Folded {
            changed,
            failure: None,
        }
    }

    fn fold_chat(&mut self, seq: Seq, kind: &EventKind) -> bool {
        let Some(chat) = kind.chat().cloned() else {
            return false;
        };
        match kind {
            EventKind::ChatClosed { .. } => return self.chats.remove(&chat).is_some(),
            EventKind::ChatOpened { state, .. } => {
                self.chats
                    .entry(chat.clone())
                    .or_insert_with(|| RemoteChat::new(ChatMirror::new((**state).clone())));
            }
            _ => {}
        }
        let Some(known) = self.chats.get_mut(&chat) else {
            return false;
        };
        known.mirror.apply(kind);
        known.seq = Some(seq);
        known.heard_at = Some(SystemTime::now());
        true
    }

    pub(super) fn refresh_hosted(&mut self) {
        let chats = &self.chats;
        self.hosted = self
            .environments
            .all()
            .iter()
            .flat_map(|data| {
                hosted_summaries(data, |terminal| {
                    let id = AgentRef {
                        environment: data.name.clone(),
                        terminal,
                    };
                    let known = chats.get(&id)?;
                    Some(ChatFacts {
                        state: &known.mirror.state,
                        asks: known.mirror.asks.len(),
                        last_activity: known.heard_at,
                        seq: known.seq,
                    })
                })
            })
            .collect();
    }
}
