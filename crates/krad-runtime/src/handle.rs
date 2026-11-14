use futures::channel::{mpsc, oneshot};
use krad_chat::acp::OpenBuffers;
use krad_chat::OpencodeServer;
use krad_domain::{EnvironmentName, Host};
use krad_protocol::{
    AgentRef, Command, Hello, Reply, RequestId, RuntimeCommand, RuntimeRun, Scope, Seq, Snapshot,
    TerminalCommand, Welcome, WriteBy,
};
use krad_session::{AnsiPalette, SearchDirection};
use krad_store::Secrets;
use std::hash::{BuildHasher as _, RandomState};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError};
use std::sync::Arc;
use std::time::SystemTime;

use crate::bus::{BusAccess, DelegationReceiver, DELEGATION_BUFFER};
use crate::chats::{ChatStart, OfferedBuffers};
use crate::hosts::HostLinks;
use crate::input::Input;
use crate::launches::LaunchedViews;
use crate::opencode::OpencodeDirectory;
use crate::terminals::{Shared, TerminalAction, TerminalLaunch, TerminalView};

mod hosts;
mod waits;

pub use waits::{EventReceiver, Pending};

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
        (links, opencode, views, secrets): (HostLinks, OpencodeDirectory, LaunchedViews, Secrets),
    ) -> Self {
        Self {
            _alive: Arc::new(Alive(inbox.clone())),
            inbox,
            bus,
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
    pub fn has_stopped(&self) -> bool {
        matches!(
            self.inbox.try_send(Input::Wake),
            Err(TrySendError::Disconnected(_))
        )
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

    pub fn settle(&self) -> Pending<Reply> {
        self.send(
            self.next_request(),
            Command::Runtime(RuntimeCommand::Settle),
        )
    }

    #[must_use]
    pub fn bus(&self) -> Option<&BusAccess> {
        self.bus.as_deref()
    }

    pub fn withdraw_discovery(&self) {
        if let Some(access) = self.bus() {
            crate::bus::withdraw_discovery(access);
        }
    }

    pub fn delegate_bus(&self) -> DelegationReceiver {
        let (delegate, receiver) = mpsc::channel(DELEGATION_BUFFER);
        self.deliver(Input::Delegate(delegate));
        DelegationReceiver(receiver)
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

    pub(crate) fn deliver(&self, input: Input) {
        if self.inbox.send(input).is_err() {
            tracing::debug!("input reached a stopped runtime");
        }
    }
}
