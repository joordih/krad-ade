use futures::channel::{mpsc, oneshot};
use futures::{Stream, StreamExt as _};
use krad_protocol::Feed;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use super::RuntimeStopped;

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
