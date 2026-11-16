use futures::channel::{mpsc, oneshot};
use futures::SinkExt as _;
use krad_domain::{CloudRunnerId, TerminalId};
use krad_protocol::{
    read_frame, DelegatedRequest, Feed, Frame, Message, PairingAnswer, Reply, RequestId, Snapshot,
    TerminalBytes, Welcome,
};
use std::sync::Arc;

use crate::client::ClientError;
use std::collections::HashMap;
use std::io::Read;
use std::sync::{Mutex, MutexGuard, PoisonError};

pub(super) enum Awaiting {
    Reply(oneshot::Sender<Result<Reply, Reply>>),
    Snapshot(oneshot::Sender<Result<Snapshot, Reply>>),
    Pairing(oneshot::Sender<Result<PairingAnswer, Reply>>),
}

#[derive(Default)]
struct Expected {
    answers: HashMap<RequestId, Awaiting>,
    feed: Option<mpsc::Sender<Feed>>,
    terminals: HashMap<TerminalId, mpsc::Sender<Vec<u8>>>,
    delegate: Option<mpsc::Sender<DelegatedRequest>>,
    relays: HashMap<CloudRunnerId, Arc<Waiting>>,
    welcome: Option<oneshot::Sender<Result<Welcome, ClientError>>>,
}

#[derive(Default)]
pub(super) struct Waiting(Mutex<Expected>);

impl Waiting {
    fn expected(&self) -> MutexGuard<'_, Expected> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn wait(&self, id: RequestId, awaiting: Awaiting) {
        self.expected().answers.insert(id, awaiting);
    }

    pub(super) fn forget(&self, id: RequestId) {
        self.expected().answers.remove(&id);
    }

    pub(super) fn follow(&self, feed: mpsc::Sender<Feed>) {
        self.expected().feed = Some(feed);
    }

    pub(super) fn stop_following(&self) {
        self.expected().feed = None;
    }

    pub(super) fn watch(&self, terminal: TerminalId, bytes: mpsc::Sender<Vec<u8>>) {
        self.expected().terminals.insert(terminal, bytes);
    }

    pub(super) fn delegate_to(&self, delegate: mpsc::Sender<DelegatedRequest>) {
        self.expected().delegate = Some(delegate);
    }

    fn delegate(&self) -> Option<mpsc::Sender<DelegatedRequest>> {
        self.expected().delegate.clone()
    }

    fn stop_delegating(&self) {
        self.expected().delegate = None;
    }

    pub(super) fn relay_to(&self, runner: CloudRunnerId, relay: Arc<Self>) {
        self.expected().relays.insert(runner, relay);
    }

    fn relay(&self, runner: CloudRunnerId) -> Option<Arc<Self>> {
        self.expected().relays.get(&runner).cloned()
    }

    pub(super) fn expect_welcome(&self, welcome: oneshot::Sender<Result<Welcome, ClientError>>) {
        self.expected().welcome = Some(welcome);
    }

    fn welcomed(&self) -> Option<oneshot::Sender<Result<Welcome, ClientError>>> {
        self.expected().welcome.take()
    }

    fn terminal(&self, terminal: TerminalId) -> Option<mpsc::Sender<Vec<u8>>> {
        self.expected().terminals.get(&terminal).cloned()
    }

    fn unwatch(&self, terminal: TerminalId) {
        self.expected().terminals.remove(&terminal);
    }

    fn take(&self, id: RequestId) -> Option<Awaiting> {
        self.expected().answers.remove(&id)
    }

    fn feed(&self) -> Option<mpsc::Sender<Feed>> {
        self.expected().feed.clone()
    }

    fn close(&self) {
        let mut expected = self.expected();
        expected.answers.clear();
        expected.feed = None;
        expected.terminals.clear();
        expected.delegate = None;
        expected.welcome = None;
        let relays: Vec<Arc<Self>> = expected.relays.drain().map(|(_, relay)| relay).collect();
        drop(expected);
        for relay in relays {
            relay.close();
        }
    }
}

fn hand_reply(awaiting: Awaiting, reply: Reply) {
    let handed = match awaiting {
        Awaiting::Reply(sender) => sender.send(Ok(reply)).is_ok(),
        Awaiting::Snapshot(sender) => sender.send(Err(reply)).is_ok(),
        Awaiting::Pairing(sender) => sender.send(Err(reply)).is_ok(),
    };
    if !handed {
        tracing::debug!("a runner reply came after its caller gave up");
    }
}

fn take_relayed(runner: CloudRunnerId, message: Message, waiting: &Waiting) {
    let Some(relay) = waiting.relay(runner) else {
        return;
    };
    let message = match message {
        Message::Welcome(welcome) => match relay.welcomed() {
            Some(sender) => return drop(sender.send(Ok(welcome))),
            None => Message::Welcome(welcome),
        },
        Message::Refused { code, message } => match relay.welcomed() {
            Some(sender) => return drop(sender.send(Err(ClientError::Refused { code, message }))),
            None => Message::Refused { code, message },
        },
        other => other,
    };
    take(message, &relay);
}

fn take(message: Message, waiting: &Waiting) {
    match message {
        Message::ViaRunner { runner, message } => take_relayed(runner, *message, waiting),
        Message::Reply { id, reply } => {
            if let Some(awaiting) = waiting.take(id) {
                hand_reply(awaiting, reply);
            }
        }
        Message::Snapshot { id, snapshot } => {
            if let Some(Awaiting::Snapshot(sender)) = waiting.take(id) {
                drop(sender.send(Ok(*snapshot)));
            }
        }
        Message::Paired { id, answer } => {
            if let Some(Awaiting::Pairing(sender)) = waiting.take(id) {
                drop(sender.send(Ok(answer)));
            }
        }
        Message::Feed(feed) => {
            if let Some(mut sender) = waiting.feed() {
                if futures::executor::block_on(sender.send(feed)).is_err() {
                    waiting.stop_following();
                }
            }
        }
        Message::Delegated(delegated) => {
            if let Some(mut sender) = waiting.delegate() {
                if futures::executor::block_on(sender.send(*delegated)).is_err() {
                    waiting.stop_delegating();
                }
            }
        }
        Message::Refused { code, .. } => {
            tracing::debug!(?code, "the runner refused a message");
        }
        Message::Hello(_)
        | Message::Welcome(_)
        | Message::Subscribe { .. }
        | Message::Command { .. }
        | Message::AskSnapshot { .. }
        | Message::Pairing { .. }
        | Message::TakeDelegations
        | Message::Unknown => {}
    }
}

fn take_terminal(bytes: TerminalBytes, waiting: &Waiting) {
    let Some(mut sink) = waiting.terminal(bytes.terminal) else {
        return;
    };
    if futures::executor::block_on(sink.send(bytes.bytes)).is_err() {
        waiting.unwatch(bytes.terminal);
    }
}

pub(super) fn read_all(input: &mut impl Read, waiting: &Waiting) {
    loop {
        match read_frame(input) {
            Ok(Some(Frame::Message(message))) => take(*message, waiting),
            Ok(Some(Frame::Terminal(bytes))) => take_terminal(bytes, waiting),
            Ok(Some(Frame::RunnerTerminal { runner, bytes })) => {
                if let Some(relay) = waiting.relay(runner) {
                    take_terminal(bytes, &relay);
                }
            }
            Ok(None) => break,
            Err(error) => {
                tracing::debug!(%error, "the runner connection ended");
                break;
            }
        }
    }
    waiting.close();
}
