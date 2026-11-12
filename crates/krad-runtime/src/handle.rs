use futures::channel::{mpsc, oneshot};
use futures::{Stream, StreamExt as _};
use krad_agents::{AgentInfo, AgentKey, StateReason};
use krad_chat::Runner;
use krad_domain::{AgentPreset, Host, ToolDecision, UsageBudget};
use krad_protocol::{
    AskId, BusCommand, Command, Feed, HealthPlace, Reply, RequestId, Scope, Snapshot,
};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::Arc;
use std::task::{Context, Poll};

use crate::bus::{BusAccess, DelegationReceiver, DELEGATION_BUFFER};
use crate::harness_health::HarnessProbe;
use crate::input::Input;
use crate::usage::UsagePlace;

const SUBSCRIBER_BUFFER: usize = 256;

#[derive(Clone, Copy, PartialEq, Eq, Debug, thiserror::Error)]
#[error("the runtime has stopped")]
pub struct RuntimeStopped;

#[derive(Debug)]
struct Alive(SyncSender<Input>);

impl Drop for Alive {
    fn drop(&mut self) {
        if self.0.send(Input::Stop).is_err() {
            tracing::trace!("the runtime had already stopped");
        }
    }
}

#[derive(Clone, Debug)]
pub struct RuntimeHandle {
    inbox: SyncSender<Input>,
    bus: Option<Arc<BusAccess>>,
    usage_budget: Arc<UsageBudget>,
    requests: Arc<AtomicU64>,
    _alive: Arc<Alive>,
}

impl RuntimeHandle {
    pub(crate) fn new(
        inbox: SyncSender<Input>,
        bus: Option<Arc<BusAccess>>,
        usage_budget: Arc<UsageBudget>,
    ) -> Self {
        Self {
            _alive: Arc::new(Alive(inbox.clone())),
            inbox,
            bus,
            usage_budget,
            requests: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn send(&self, id: RequestId, command: Command) -> Pending<Reply> {
        let (reply, answer) = oneshot::channel();
        self.deliver(Input::Command { id, command, reply });
        Pending(answer)
    }

    pub fn post(&self, command: Command) {
        let (reply, _unheard) = oneshot::channel();
        let id = self.next_request();
        self.deliver(Input::Command { id, command, reply });
    }

    #[must_use]
    pub fn next_request(&self) -> RequestId {
        RequestId(self.requests.fetch_add(1, Ordering::Relaxed) + 1)
    }

    pub fn subscribe(&self) -> EventReceiver {
        let (feed, receiver) = mpsc::channel(SUBSCRIBER_BUFFER);
        self.deliver(Input::Subscribe(feed));
        EventReceiver(receiver)
    }

    pub fn snapshot(&self, scope: Scope) -> Pending<Snapshot> {
        let (reply, answer) = oneshot::channel();
        self.deliver(Input::Snapshot { scope, reply });
        Pending(answer)
    }

    pub fn settle(&self) -> Pending<()> {
        let (reply, answer) = oneshot::channel();
        self.deliver(Input::Settle(reply));
        Pending(answer)
    }

    #[must_use]
    pub fn bus(&self) -> Option<&BusAccess> {
        self.bus.as_deref()
    }

    pub fn delegate_bus(&self) -> DelegationReceiver {
        let (delegate, receiver) = mpsc::channel(DELEGATION_BUFFER);
        self.deliver(Input::Delegate(delegate));
        DelegationReceiver(receiver)
    }

    pub fn observe_agent(&self, key: AgentKey, seen: Option<AgentInfo>, hint: Option<StateReason>) {
        self.deliver(Input::ObserveAgent { key, seen, hint });
    }

    pub fn reconcile_agents(&self, environments: Vec<String>, agents: Vec<AgentInfo>) {
        self.deliver(Input::ReconcileAgents {
            environments,
            agents,
        });
    }

    pub fn check_harnesses(
        &self,
        place: HealthPlace,
        probes: Vec<(AgentPreset, Option<HarnessProbe>)>,
    ) {
        self.deliver(Input::CheckHarnesses { place, probes });
    }

    pub fn place_usage(&self, place: Option<UsagePlace>) {
        self.deliver(Input::PlaceUsage(place));
    }

    pub fn list_models(&self, host: Host, harness: AgentPreset, runner: Runner, cwd: PathBuf) {
        self.deliver(Input::ListModels {
            host,
            harness,
            runner,
            cwd,
        });
    }

    #[must_use]
    pub fn usage_budget(&self) -> Arc<UsageBudget> {
        Arc::clone(&self.usage_budget)
    }

    #[must_use]
    pub fn held_ask(&self, ask: AskId) -> HeldAsk {
        HeldAsk {
            ask,
            runtime: self.clone(),
        }
    }

    fn deliver(&self, input: Input) {
        if self.inbox.send(input).is_err() {
            tracing::debug!("input reached a stopped runtime");
        }
    }
}

#[derive(Clone, Debug)]
pub struct HeldAsk {
    pub ask: AskId,
    runtime: RuntimeHandle,
}

impl HeldAsk {
    pub fn decide(self, decision: ToolDecision) {
        self.runtime.post(Command::Bus(BusCommand::AnswerAsk {
            ask: self.ask,
            decision,
        }));
    }
}

#[must_use]
#[derive(Debug)]
pub struct Pending<T>(oneshot::Receiver<T>);

impl<T> Future for Pending<T> {
    type Output = Result<T, RuntimeStopped>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.0)
            .poll(cx)
            .map(|answer| answer.map_err(|_| RuntimeStopped))
    }
}

#[must_use]
#[derive(Debug)]
pub struct EventReceiver(mpsc::Receiver<Feed>);

impl Stream for EventReceiver {
    type Item = Feed;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Feed>> {
        self.0.poll_next_unpin(cx)
    }
}
