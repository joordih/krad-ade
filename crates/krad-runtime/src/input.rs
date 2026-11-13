use futures::channel::{mpsc, oneshot};
use krad_agents::{AgentInfo, AgentKey, Incoming, Response, StateReason};
use krad_chat::{LiveEvent, LiveSession, Runner};
use krad_domain::{AgentPreset, CloudRunnerId, EnvironmentName, Host, RunnerCli};
use krad_host::{HostLink, MachineChannel};
use krad_protocol::{
    AgentRef, ChatPage, Command, Feed, HealthPlace, Reply, RequestId, Scope, Snapshot, WebhookTest,
};
use krad_session::AnsiPalette;
use std::path::PathBuf;
use std::sync::Arc;

use crate::bus::{Delegated, DelegationId, Mail};
use crate::chats::{ChatBrief, ChatStart, IndexAnswer, LiveStarted, OfferedBuffers};
use crate::harness_health::HarnessProbe;
use crate::jobs::JobDone;
use crate::kraddies::DigestReply;
use crate::opencode::OpencodeInput;
use crate::terminals::{Shared, TerminalAction, TerminalLaunch};
use crate::usage::UsagePlace;

pub(crate) struct LiveStart {
    pub(crate) chat: AgentRef,
    pub(crate) harness: AgentPreset,
    pub(crate) brief: ChatBrief,
    pub(crate) session: LiveSession,
    pub(crate) sink: Box<dyn FnMut(LiveEvent) + Send>,
    pub(crate) reply: oneshot::Sender<LiveStarted>,
}

impl std::fmt::Debug for LiveStart {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LiveStart")
            .field("harness", &self.harness)
            .field("session", &self.session)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub(crate) enum RunnerInput {
    OpenSignIn {
        terminal: AgentRef,
        runner: CloudRunnerId,
        cli: RunnerCli,
        shared: Arc<Shared>,
    },
    OpenClaudeSetup {
        terminal: AgentRef,
        shared: Arc<Shared>,
    },
}

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
    SettingsNotSaved(String),
    KraddieNotSaved(String),
    DigestHeard {
        chat: AgentRef,
        reply: DigestReply,
    },
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
    Runner(RunnerInput),
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
    MentionIndex {
        place: (HostLink, PathBuf),
        reply: oneshot::Sender<IndexAnswer>,
    },
    ForgetMentionIndexes,
    OfferBuffers {
        environment: EnvironmentName,
        buffers: OfferedBuffers,
    },
    StartLiveChat(Box<LiveStart>),
    Opencode(OpencodeInput),
    Wake,
    Tick,
    Stop,
}
