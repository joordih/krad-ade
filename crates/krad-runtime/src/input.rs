use futures::channel::{mpsc, oneshot};
use krad_agents::{AgentInfo, AgentKey, Incoming, Response, StateReason};
use krad_protocol::{Command, Feed, Reply, RequestId, Scope, Snapshot, WebhookTest};

use crate::bus::{Delegated, DelegationId, Mail};

#[derive(Debug)]
pub(crate) enum Input {
    Command {
        id: RequestId,
        command: Command,
        reply: oneshot::Sender<Reply>,
    },
    Subscribe(mpsc::Sender<Feed>),
    Snapshot {
        scope: Scope,
        reply: oneshot::Sender<Snapshot>,
    },
    Bus(Incoming),
    BusRespond {
        delegation: DelegationId,
        response: Response,
    },
    BusMail {
        delegation: DelegationId,
        mail: Mail,
    },
    BusDropped(DelegationId),
    Delegate(mpsc::Sender<Delegated>),
    ObserveAgent {
        key: AgentKey,
        seen: Option<AgentInfo>,
        hint: Option<StateReason>,
    },
    ReconcileAgents {
        environments: Vec<String>,
        agents: Vec<AgentInfo>,
    },
    WebhookTested(WebhookTest),
    Tick,
    Stop,
}
