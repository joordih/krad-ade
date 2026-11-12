use futures::channel::{mpsc, oneshot};
use krad_agents::{AgentInfo, AgentKey, Incoming, Response, StateReason};
use krad_chat::Runner;
use krad_domain::{AgentPreset, Host};
use krad_protocol::{Command, Feed, HealthPlace, Reply, RequestId, Scope, Snapshot, WebhookTest};
use std::path::PathBuf;

use crate::bus::{Delegated, DelegationId, Mail};
use crate::harness_health::HarnessProbe;
use crate::jobs::JobDone;
use crate::usage::UsagePlace;

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
    CheckHarnesses {
        place: HealthPlace,
        probes: Vec<(AgentPreset, Option<HarnessProbe>)>,
    },
    PlaceUsage(Option<UsagePlace>),
    ListModels {
        host: Host,
        harness: AgentPreset,
        runner: Runner,
        cwd: PathBuf,
    },
    JobDone(JobDone),
    Settle(oneshot::Sender<()>),
    Tick,
    Stop,
}
