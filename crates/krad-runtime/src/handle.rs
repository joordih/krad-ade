use futures::channel::{mpsc, oneshot};
use krad_agents::{AgentInfo, AgentKey, StateReason};
use krad_chat::acp::OpenBuffers;
use krad_chat::{LiveEvent, LiveSession, OpencodeServer, Runner};
use krad_domain::{AgentPreset, EnvironmentName, Host, UsageBudget};
use krad_host::HostLink;
use krad_protocol::{
    AgentRef, AskId, ChatPage, Command, HealthPlace, Hello, Reply, RequestId, RuntimeRun, Scope,
    Seq, Snapshot, TerminalCommand, Welcome, WriteBy,
};
use krad_repo::{FileIndex, RepoError};
use krad_session::{AnsiPalette, SearchDirection};
use krad_store::Secrets;
use std::hash::{BuildHasher as _, RandomState};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::Arc;
use std::time::SystemTime;

use crate::bus::{BusAccess, DelegationReceiver, DELEGATION_BUFFER};
use crate::chats::{ChatBrief, ChatStart, LiveStarted, OfferedBuffers};
use crate::harness_health::HarnessProbe;
use crate::hosts::HostLinks;
use crate::input::{Input, LiveStart};
use crate::launches::LaunchedViews;
use crate::opencode::{OpencodeDirectory, OpencodeInput};
use crate::terminals::{Shared, TerminalAction, TerminalLaunch, TerminalView};
use crate::usage::UsagePlace;

mod hosts;
mod waits;

pub use waits::{EventReceiver, HeldAsk, Pending};

const SUBSCRIBER_BUFFER: usize = 256;
const BUILD: &str = env!("CARGO_PKG_VERSION");

fn fresh_run() -> RuntimeRun {
    RuntimeRun(RandomState::new().hash_one(SystemTime::now()))
}

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
    links: HostLinks,
    opencode: OpencodeDirectory,
    views: LaunchedViews,
    secrets: Secrets,
    requests: Arc<AtomicU64>,
    run: RuntimeRun,
    _alive: Arc<Alive>,
}

impl RuntimeHandle {
    pub(crate) fn new(
        inbox: SyncSender<Input>,
        bus: Option<Arc<BusAccess>>,
        usage_budget: Arc<UsageBudget>,
        (links, opencode, views, secrets): (HostLinks, OpencodeDirectory, LaunchedViews, Secrets),
    ) -> Self {
        Self {
            _alive: Arc::new(Alive(inbox.clone())),
            inbox,
            bus,
            usage_budget,
            links,
            opencode,
            views,
            secrets,
            requests: Arc::new(AtomicU64::new(0)),
            run: fresh_run(),
        }
    }

    /// # Errors
    ///
    /// Refuses a client that speaks another major version of the protocol.
    pub fn hello(&self, hello: &Hello) -> Result<Welcome, Reply> {
        hello.welcome(BUILD, self.run)
    }

    #[must_use]
    pub const fn secrets(&self) -> &Secrets {
        &self.secrets
    }

    pub fn send(&self, id: RequestId, command: Command) -> Pending<Reply> {
        let (reply, answer) = oneshot::channel();
        self.deliver(Input::Command { id, command, reply });
        Pending(answer)
    }

    pub fn post(&self, command: Command) {
        self.post_as(self.next_request(), command);
    }

    pub fn post_as(&self, id: RequestId, command: Command) {
        let (reply, _unheard) = oneshot::channel();
        self.deliver(Input::Command { id, command, reply });
    }

    #[must_use]
    pub fn next_request(&self) -> RequestId {
        RequestId(self.requests.fetch_add(1, Ordering::Relaxed) + 1)
    }

    pub fn subscribe(&self) -> EventReceiver {
        self.subscribe_since(None)
    }

    pub fn resume(&self, since: Seq) -> EventReceiver {
        self.subscribe_since(Some(since))
    }

    fn subscribe_since(&self, since: Option<Seq>) -> EventReceiver {
        let (feed, receiver) = mpsc::channel(SUBSCRIBER_BUFFER);
        self.deliver(Input::Subscribe { feed, since });
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
    pub fn open_terminal(&self, terminal: AgentRef, launch: TerminalLaunch) -> TerminalView {
        let shared = Shared::new(self.inbox.clone());
        let view = TerminalView::new(Arc::clone(&shared));
        self.deliver(Input::OpenTerminal {
            terminal,
            launch,
            shared,
        });
        view
    }

    #[must_use]
    pub fn take_launched_view(&self, terminal: &AgentRef) -> Option<TerminalView> {
        self.views.take(terminal)
    }

    pub fn open_chat(&self, chat: AgentRef, start: ChatStart) {
        self.deliver(Input::OpenChat {
            chat,
            start: Box::new(start),
        });
    }

    pub fn offer_open_buffers(&self, environment: EnvironmentName, buffers: Arc<dyn OpenBuffers>) {
        let buffers = OfferedBuffers(buffers);
        self.deliver(Input::OfferBuffers {
            environment,
            buffers,
        });
    }

    pub fn close_chat(&self, chat: AgentRef) {
        self.deliver(Input::CloseChat(chat));
    }

    pub fn chat_page(
        &self,
        chat: AgentRef,
        before: usize,
        count: usize,
    ) -> Pending<Option<ChatPage>> {
        let (reply, answer) = oneshot::channel();
        self.deliver(Input::ChatPage {
            chat,
            before,
            count,
            reply,
        });
        Pending(answer)
    }

    pub fn mention_index(
        &self,
        link: HostLink,
        root: PathBuf,
    ) -> Pending<Result<Arc<FileIndex>, Arc<RepoError>>> {
        let (reply, answer) = oneshot::channel();
        self.deliver(Input::MentionIndex {
            place: (link, root),
            reply,
        });
        Pending(answer)
    }

    pub fn forget_mention_indexes(&self) {
        self.deliver(Input::ForgetMentionIndexes);
    }

    pub fn start_live_chat(
        &self,
        (chat, harness): (AgentRef, AgentPreset),
        brief: ChatBrief,
        session: LiveSession,
        sink: impl FnMut(LiveEvent) + Send + 'static,
    ) -> Pending<LiveStarted> {
        let (reply, answer) = oneshot::channel();
        self.deliver(Input::StartLiveChat(Box::new(LiveStart {
            chat,
            harness,
            brief,
            session,
            sink: Box::new(sink),
            reply,
        })));
        Pending(answer)
    }

    pub fn start_opencode(&self, host: Host, runner: Runner, cwd: PathBuf) {
        self.deliver(Input::Opencode(OpencodeInput::Start { host, runner, cwd }));
    }

    pub fn refresh_opencode_models(&self, host: Host) {
        self.deliver(Input::Opencode(OpencodeInput::Refresh(host)));
    }

    pub fn forget_opencode(&self, host: Host) {
        self.deliver(Input::Opencode(OpencodeInput::Forget(host)));
    }

    #[must_use]
    pub fn opencode_server(&self, host: &Host) -> Option<Arc<OpencodeServer>> {
        self.opencode.server(host)
    }

    pub fn write_terminal(&self, terminal: AgentRef, bytes: Vec<u8>, by: WriteBy) {
        self.post(Command::Terminal(TerminalCommand::Write {
            terminal,
            bytes,
            by,
        }));
    }

    pub fn act_on_terminal(&self, terminal: AgentRef, action: TerminalAction) {
        self.deliver(Input::TerminalAction { terminal, action });
    }

    pub fn terminal_selection(&self, terminal: AgentRef) -> Pending<Option<String>> {
        let (text, answer) = oneshot::channel();
        self.act_on_terminal(terminal, TerminalAction::CopySelection(text));
        Pending(answer)
    }

    pub fn search_terminal(
        &self,
        terminal: AgentRef,
        query: String,
        direction: SearchDirection,
        restart: bool,
    ) -> Pending<bool> {
        let (found, answer) = oneshot::channel();
        let action = TerminalAction::Search {
            query,
            direction,
            restart,
            found,
        };
        self.act_on_terminal(terminal, action);
        Pending(answer)
    }

    pub fn terminal_lines(&self, terminal: AgentRef, lines: usize) -> Pending<Option<Vec<String>>> {
        let (tail, answer) = oneshot::channel();
        self.act_on_terminal(terminal, TerminalAction::RecentLines { lines, tail });
        Pending(answer)
    }

    pub fn terminal_palette(&self, palette: AnsiPalette) {
        self.deliver(Input::TerminalPalette(palette));
    }

    #[must_use]
    pub fn held_ask(&self, ask: AskId) -> HeldAsk {
        HeldAsk {
            ask,
            runtime: self.clone(),
        }
    }

    pub(crate) fn deliver(&self, input: Input) {
        if self.inbox.send(input).is_err() {
            tracing::debug!("input reached a stopped runtime");
        }
    }
}
