use crate::protocol::{
    AgentChangeKind, AgentEvent, AgentInfo, AgentSnapshot, AgentSubscription, Heartbeat, Response,
    StateReason, StreamId, TerminalState, Timestamp,
};
use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

pub const EVENT_RING_CAPACITY: usize = 1024;
pub const SUBSCRIBER_QUEUE: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AgentKey {
    pub environment: String,
    pub terminal: u64,
}

impl AgentKey {
    #[must_use]
    pub fn new(environment: &str, terminal: u64) -> Self {
        Self {
            environment: environment.to_owned(),
            terminal,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct AgentFeed {
    state: Arc<Mutex<FeedState>>,
}

#[derive(Debug)]
struct FeedState {
    stream: StreamId,
    seq: u64,
    agents: BTreeMap<AgentKey, AgentInfo>,
    ring: VecDeque<AgentEvent>,
    subscribers: Vec<Subscriber>,
    next_subscriber: u64,
}

impl Default for FeedState {
    fn default() -> Self {
        Self {
            stream: StreamId(crate::token::random_hex()),
            seq: 0,
            agents: BTreeMap::new(),
            ring: VecDeque::with_capacity(EVENT_RING_CAPACITY),
            subscribers: Vec::new(),
            next_subscriber: 0,
        }
    }
}

#[derive(Debug)]
struct Subscriber {
    id: u64,
    subscription: AgentSubscription,
    lines: SyncSender<Response>,
    lagged: Arc<AtomicBool>,
}

#[derive(Debug)]
pub(crate) struct Subscription {
    pub(crate) id: u64,
    pub(crate) opening: Vec<Response>,
    pub(crate) lines: Receiver<Response>,
    pub(crate) lagged: Arc<AtomicBool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Change {
    pub(crate) kind: AgentChangeKind,
    pub(crate) previous: Option<TerminalState>,
    pub(crate) state: TerminalState,
}

pub(crate) fn change(before: Option<&AgentInfo>, after: Option<&AgentInfo>) -> Option<Change> {
    let (kind, previous, state) = match (before, after) {
        (None, None) => return None,
        (None, Some(after)) => (AgentChangeKind::Opened, None, after.state),
        (Some(before), None) => (
            AgentChangeKind::Closed,
            Some(before.state),
            TerminalState::Closed,
        ),
        (Some(before), Some(after)) if before.state == after.state => return None,
        (Some(before), Some(after)) => (
            AgentChangeKind::StateChanged,
            Some(before.state),
            after.state,
        ),
    };
    Some(Change {
        kind,
        previous,
        state,
    })
}

fn tracked(key: &AgentKey, mut agent: AgentInfo) -> AgentInfo {
    agent.environment = Some(key.environment.clone());
    agent.terminal = key.terminal;
    agent.is_self = false;
    agent
}

impl AgentFeed {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, FeedState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn observe(
        &self,
        key: AgentKey,
        seen: Option<AgentInfo>,
        hint: Option<StateReason>,
        at: Timestamp,
    ) {
        let seen = seen.map(|agent| tracked(&key, agent));
        self.lock().record(key, seen, hint, at);
    }

    pub fn reconcile(&self, environments: &[String], seen: Vec<AgentInfo>, at: Timestamp) {
        let seen: BTreeMap<AgentKey, AgentInfo> = seen
            .into_iter()
            .filter_map(|agent| {
                let key = AgentKey::new(agent.environment.as_deref()?, agent.terminal);
                Some((key.clone(), tracked(&key, agent)))
            })
            .collect();
        let mut state = self.lock();
        let gone: Vec<AgentKey> = state
            .agents
            .keys()
            .filter(|key| !seen.contains_key(*key))
            .cloned()
            .collect();
        for key in gone {
            let hint = (!environments.contains(&key.environment))
                .then_some(StateReason::EnvironmentClosed);
            state.record(key, None, hint, at);
        }
        for (key, agent) in seen {
            state.record(key, Some(agent), None, at);
        }
    }

    pub(crate) fn subscribe(&self, subscription: AgentSubscription, at: Timestamp) -> Subscription {
        let mut state = self.lock();
        let opening = state.opening(&subscription, at);
        let (lines, receiver) = sync_channel(SUBSCRIBER_QUEUE);
        let lagged = Arc::new(AtomicBool::new(false));
        let id = state.next_subscriber;
        state.next_subscriber += 1;
        state.subscribers.push(Subscriber {
            id,
            subscription,
            lines,
            lagged: Arc::clone(&lagged),
        });
        Subscription {
            id,
            opening,
            lines: receiver,
            lagged,
        }
    }

    pub(crate) fn heartbeat(&self, at: Timestamp) -> Heartbeat {
        self.lock().heartbeat(at)
    }

    pub(crate) fn unsubscribe(&self, id: u64) {
        self.lock()
            .subscribers
            .retain(|subscriber| subscriber.id != id);
    }
}

impl FeedState {
    fn record(
        &mut self,
        key: AgentKey,
        after: Option<AgentInfo>,
        hint: Option<StateReason>,
        at: Timestamp,
    ) {
        let before = self.agents.get(&key);
        let Some(change) = change(before, after.as_ref()) else {
            if let Some(after) = after {
                self.agents.insert(key, after);
            }
            return;
        };
        let Some(shown) = after.as_ref().or(before) else {
            return;
        };
        self.seq += 1;
        let event = AgentEvent {
            seq: self.seq,
            at,
            kind: change.kind,
            environment: key.environment.clone(),
            terminal: key.terminal,
            title: shown.title.clone(),
            group: shown.group.clone(),
            harness: shown.harness,
            previous: change.previous,
            state: change.state,
            coarse_state: change.state.into(),
            reason: hint.or_else(|| StateReason::inferred(change.previous, change.state)),
        };
        match after {
            Some(after) => {
                self.agents.insert(key, after);
            }
            None => {
                self.agents.remove(&key);
            }
        }
        self.fan_out(&event);
        if self.ring.len() == EVENT_RING_CAPACITY {
            self.ring.pop_front();
        }
        self.ring.push_back(event);
    }

    fn fan_out(&mut self, event: &AgentEvent) {
        self.subscribers.retain(|subscriber| {
            if !subscriber.subscription.filter.admits_event(event) {
                return true;
            }
            match subscriber
                .lines
                .try_send(Response::AgentEvent(Box::new(event.clone())))
            {
                Ok(()) => true,
                Err(TrySendError::Full(_)) => {
                    subscriber.lagged.store(true, Ordering::Release);
                    false
                }
                Err(TrySendError::Disconnected(_)) => false,
            }
        });
    }

    fn opening(&self, subscription: &AgentSubscription, at: Timestamp) -> Vec<Response> {
        if let Some(missed) = self.replay(subscription) {
            return std::iter::once(Response::Heartbeat(self.heartbeat(at)))
                .chain(
                    missed
                        .into_iter()
                        .map(|event| Response::AgentEvent(Box::new(event))),
                )
                .collect();
        }
        vec![Response::AgentSnapshot(Box::new(AgentSnapshot {
            stream: self.stream.clone(),
            seq: self.seq,
            at,
            resync: subscription.since_seq.is_some(),
            agents: self
                .agents
                .values()
                .filter(|agent| subscription.filter.admits_agent(agent))
                .cloned()
                .collect(),
        }))]
    }

    fn replay(&self, subscription: &AgentSubscription) -> Option<Vec<AgentEvent>> {
        let since = subscription.since_seq?;
        let stream = subscription.stream.as_ref()?;
        if *stream != self.stream || since > self.seq {
            return None;
        }
        let oldest = self.ring.front().map_or(self.seq + 1, |event| event.seq);
        if since + 1 < oldest {
            return None;
        }
        Some(
            self.ring
                .iter()
                .filter(|event| event.seq > since)
                .filter(|event| subscription.filter.admits_event(event))
                .cloned()
                .collect(),
        )
    }

    fn heartbeat(&self, at: Timestamp) -> Heartbeat {
        Heartbeat {
            stream: self.stream.clone(),
            seq: self.seq,
            at,
        }
    }
}

#[cfg(test)]
mod tests;
