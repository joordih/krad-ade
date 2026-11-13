use futures::channel::{mpsc, oneshot};
use futures::{Stream, StreamExt as _};
use krad_domain::{AskAnswer, ToolDecision};
use krad_protocol::{AskId, BusCommand, Command, Feed};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use super::{RuntimeHandle, RuntimeStopped};

#[derive(Clone, Debug)]
pub struct HeldAsk {
    pub ask: AskId,
    pub(crate) runtime: RuntimeHandle,
}

impl HeldAsk {
    pub fn decide(self, decision: ToolDecision) {
        self.runtime.post(Command::Bus(BusCommand::AnswerAsk {
            ask: self.ask,
            decision,
        }));
    }

    pub fn answer(self, answer: AskAnswer) {
        self.runtime.post(Command::Bus(BusCommand::Answer {
            ask: self.ask,
            answer,
        }));
    }
}

#[must_use]
#[derive(Debug)]
pub struct Pending<T>(pub(crate) oneshot::Receiver<T>);

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
pub struct EventReceiver(pub(crate) mpsc::Receiver<Feed>);

impl Stream for EventReceiver {
    type Item = Feed;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Feed>> {
        self.0.poll_next_unpin(cx)
    }
}
