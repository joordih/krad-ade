use futures::channel::{mpsc, oneshot};
use futures::{Stream, StreamExt as _};
use krad_protocol::{Command, Feed, Reply, RequestId, Scope, Snapshot};
use std::future::Future;
use std::pin::Pin;
use std::sync::mpsc::SyncSender;
use std::task::{Context, Poll};

use crate::input::Input;

const SUBSCRIBER_BUFFER: usize = 256;

#[derive(Clone, Copy, PartialEq, Eq, Debug, thiserror::Error)]
#[error("the runtime has stopped")]
pub struct RuntimeStopped;

#[derive(Clone, Debug)]
pub struct RuntimeHandle {
    inbox: SyncSender<Input>,
}

impl RuntimeHandle {
    pub(crate) fn new(inbox: SyncSender<Input>) -> Self {
        Self { inbox }
    }

    pub fn send(&self, id: RequestId, command: Command) -> Pending<Reply> {
        let (reply, answer) = oneshot::channel();
        self.deliver(Input::Command { id, command, reply });
        Pending(answer)
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

    fn deliver(&self, input: Input) {
        if self.inbox.send(input).is_err() {
            tracing::debug!("input reached a stopped runtime");
        }
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
