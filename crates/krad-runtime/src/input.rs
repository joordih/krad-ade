use futures::channel::{mpsc, oneshot};
use krad_agents::{Incoming, Response};
use krad_chat::{LiveEvent, LiveSession};
use krad_domain::{AgentPreset, CloudRunnerId, EnvironmentName, RunnerCli};
use krad_host::MachineChannel;
use krad_protocol::{AgentRef, Command, Feed, Reply, RequestId, Scope, Seq, Snapshot, WebhookTest};
use krad_session::AnsiPalette;
use std::sync::Arc;

use crate::bus::{Delegated, DelegationId, Mail};
use crate::chats::{ChatBrief, ChatStart, LiveStarted, OfferedBuffers};
use crate::jobs::JobDone;
use crate::kraddies::DigestReply;
use crate::opencode::OpencodeInput;
use crate::terminals::{Shared, TerminalAction, TerminalLaunch};

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
    Subscribe {
        feed: mpsc::Sender<Feed>,
        since: Option<Seq>,
    },
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
    WebhookTested(WebhookTest),
    JobDone(JobDone),
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
