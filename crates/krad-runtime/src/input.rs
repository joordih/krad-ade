use futures::channel::{mpsc, oneshot};
use krad_agents::{AgentInfo, AgentKey, Incoming, Response, StateReason};
use krad_chat::Runner;
use krad_domain::{AgentPreset, EnvironmentName, Host};
use krad_host::MachineChannel;
use krad_protocol::{
    AgentRef, ChatPage, Command, Feed, HealthPlace, Reply, RequestId, Scope, Snapshot, WebhookTest,
};
use krad_session::AnsiPalette;
use std::path::PathBuf;
use std::sync::Arc;

use crate::bus::{Delegated, DelegationId, Mail};
use crate::chats::ChatStart;
use crate::harness_health::HarnessProbe;
use crate::jobs::JobDone;
use crate::terminals::{Shared, TerminalAction, TerminalLaunch};
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
    OpenTerminal {
        terminal: AgentRef,
        launch: TerminalLaunch,
        shared: Arc<Shared>,
    },
    TerminalAction {
        terminal: AgentRef,
        action: TerminalAction,
    },
    TerminalPalette(AnsiPalette),
    OpenSignIn {
        terminal: AgentRef,
        shared: Arc<Shared>,
    },
    MachineChannel {
        environment: EnvironmentName,
        channel: Result<MachineChannel, String>,
    },
    OpenChat {
        chat: AgentRef,
        start: Box<ChatStart>,
    },
    CloseChat(AgentRef),
    ChatPage {
        chat: AgentRef,
        before: usize,
        count: usize,
        reply: oneshot::Sender<Option<ChatPage>>,
    },
    Wake,
    Tick,
    Stop,
}
