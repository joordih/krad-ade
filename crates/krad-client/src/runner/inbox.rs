use futures::channel::{mpsc, oneshot};
use futures::SinkExt as _;
use krad_protocol::{read_frame, Feed, Frame, Message, PairingAnswer, Reply, RequestId, Snapshot};
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

fn take(message: Message, waiting: &Waiting) {
    match message {
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
        Message::Refused { code, .. } => {
            tracing::debug!(?code, "the runner refused a message");
        }
        Message::Hello(_)
        | Message::Welcome(_)
        | Message::Subscribe { .. }
        | Message::Command { .. }
        | Message::AskSnapshot { .. }
        | Message::Pairing { .. }
        | Message::Unknown => {}
    }
}

pub(super) fn read_all(input: &mut impl Read, waiting: &Waiting) {
    loop {
        match read_frame(input) {
            Ok(Some(Frame::Message(message))) => take(*message, waiting),
            Ok(Some(Frame::Terminal(_))) => {}
            Ok(None) => break,
            Err(error) => {
                tracing::debug!(%error, "the runner connection ended");
                break;
            }
        }
    }
    waiting.close();
}
