use futures::channel::mpsc;
use futures::future::BoxFuture;
use futures::{Stream as FeedStream, StreamExt as _};
use krad_protocol::{write_frame, Feed, Frame, Message, TerminalBytes};
use std::io::{BufWriter, Write};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::thread;

use super::listener::Stream;

const OUTBOX_CAPACITY: usize = 256;
const WRITER_THREAD: &str = "krad-runner-writer";
const ANSWERS_THREAD: &str = "krad-runner-answers";
const FEED_THREAD: &str = "krad-runner-feed";

#[derive(Clone)]
pub(crate) struct Outbox(SyncSender<Frame>);

impl Outbox {
    pub(crate) fn start(stream: impl Stream) -> Option<Self> {
        let (sender, receiver) = sync_channel(OUTBOX_CAPACITY);
        spawn(WRITER_THREAD, move || write_all(stream, &receiver))?;
        Some(Self(sender))
    }

    pub(crate) fn send(&self, message: Message) -> bool {
        self.0.send(Frame::Message(Box::new(message))).is_ok()
    }

    pub(crate) fn send_frame(&self, frame: Frame) -> bool {
        self.0.send(frame).is_ok()
    }

    pub(crate) fn send_terminal(&self, bytes: TerminalBytes) -> bool {
        self.0.send(Frame::Terminal(bytes)).is_ok()
    }

    pub(crate) fn follow(&self, feed: impl FeedStream<Item = Feed> + Send + Unpin + 'static) {
        let outbox = self.clone();
        spawn(FEED_THREAD, move || {
            futures::executor::block_on(forward_feed(feed, &outbox));
        });
    }
}

async fn forward_feed(mut feed: impl FeedStream<Item = Feed> + Unpin, outbox: &Outbox) {
    while let Some(feed) = feed.next().await {
        if !outbox.send(Message::Feed(feed)) {
            return;
        }
    }
}

fn write_all(stream: impl Stream, receiver: &Receiver<Frame>) {
    let mut out = BufWriter::new(stream);
    for frame in receiver {
        let written = write_frame(&mut out, &frame)
            .map_err(|error| error.to_string())
            .and_then(|()| out.flush().map_err(|error| error.to_string()));
        if let Err(error) = written {
            tracing::debug!(%error, "a runtime connection stopped taking frames");
            return;
        }
    }
}

fn spawn(name: &str, work: impl FnOnce() + Send + 'static) -> Option<()> {
    thread::Builder::new()
        .name(name.to_owned())
        .spawn(work)
        .inspect_err(|error| tracing::warn!(%error, "a runtime connection thread did not start"))
        .ok()
        .map(drop)
}

pub(crate) struct Answers(mpsc::UnboundedSender<BoxFuture<'static, Message>>);

impl Answers {
    pub(crate) fn start(outbox: Outbox) -> Option<Self> {
        let (sender, receiver) = mpsc::unbounded::<BoxFuture<'static, Message>>();
        spawn(ANSWERS_THREAD, move || {
            futures::executor::block_on(receiver.for_each_concurrent(None, |answer| {
                let outbox = outbox.clone();
                async move {
                    outbox.send(answer.await);
                }
            }));
        })?;
        Some(Self(sender))
    }

    pub(crate) fn wait_for(&self, answer: BoxFuture<'static, Message>) {
        if self.0.unbounded_send(answer).is_err() {
            tracing::debug!("a runtime answer came after its connection closed");
        }
    }
}
