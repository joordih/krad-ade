use futures::channel::mpsc;
use futures::future::BoxFuture;
use futures::{Stream as FeedStream, StreamExt as _};
use krad_protocol::{write_frame, Feed, Frame, Message, TerminalBytes};
use std::io::{BufWriter, Write};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;

use super::listener::Stream;

const STREAMED_BACKLOG: usize = 256;
const ANSWERED_BACKLOG: usize = 65_536;
const WRITER_THREAD: &str = "krad-runner-writer";
const ANSWERS_THREAD: &str = "krad-runner-answers";
const FEED_THREAD: &str = "krad-runner-feed";

type HangUp = Box<dyn FnOnce() + Send>;

struct Backlog {
    unwritten: usize,
    closed: bool,
    hang_up: Option<HangUp>,
}

struct Pending {
    backlog: Mutex<Backlog>,
    written: Condvar,
}

impl Pending {
    fn backlog(&self) -> MutexGuard<'_, Backlog> {
        self.backlog.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn close(&self, mut backlog: MutexGuard<'_, Backlog>) {
        backlog.closed = true;
        let hang_up = backlog.hang_up.take();
        drop(backlog);
        self.written.notify_all();
        if let Some(hang_up) = hang_up {
            hang_up();
        }
    }

    fn wrote_one(&self) {
        let mut backlog = self.backlog();
        backlog.unwritten = backlog.unwritten.saturating_sub(1);
        drop(backlog);
        self.written.notify_all();
    }
}

#[derive(Clone)]
pub(crate) struct Outbox {
    frames: Sender<Frame>,
    pending: Arc<Pending>,
}

impl Outbox {
    pub(crate) fn start<S: Stream>(stream: S) -> Option<Self> {
        let hanging = stream
            .try_clone()
            .inspect_err(|error| tracing::debug!(%error, "a runtime connection was not shared"))
            .ok()?;
        let pending = Arc::new(Pending {
            backlog: Mutex::new(Backlog {
                unwritten: 0,
                closed: false,
                hang_up: Some(Box::new(move || hanging.close())),
            }),
            written: Condvar::new(),
        });
        let (frames, receiver) = channel();
        let writing = Arc::clone(&pending);
        spawn(WRITER_THREAD, move || {
            write_all(stream, &receiver, &writing);
        })?;
        Some(Self { frames, pending })
    }

    pub(crate) fn send(&self, message: Message) -> bool {
        let backlog = self.pending.backlog();
        if backlog.closed {
            return false;
        }
        if backlog.unwritten >= ANSWERED_BACKLOG {
            tracing::warn!("a runtime connection stopped reading its answers, closing it");
            self.pending.close(backlog);
            return false;
        }
        self.queue(backlog, Frame::Message(Box::new(message)))
    }

    pub(crate) fn stream(&self, message: Message) -> bool {
        self.stream_frame(Frame::Message(Box::new(message)))
    }

    pub(crate) fn stream_terminal(&self, bytes: TerminalBytes) -> bool {
        self.stream_frame(Frame::Terminal(bytes))
    }

    pub(crate) fn stream_frame(&self, frame: Frame) -> bool {
        let backlog = self.pending.backlog();
        let backlog = self
            .pending
            .written
            .wait_while(backlog, |backlog| {
                !backlog.closed && backlog.unwritten >= STREAMED_BACKLOG
            })
            .unwrap_or_else(PoisonError::into_inner);
        if backlog.closed {
            return false;
        }
        self.queue(backlog, frame)
    }

    fn queue(&self, mut backlog: MutexGuard<'_, Backlog>, frame: Frame) -> bool {
        backlog.unwritten += 1;
        drop(backlog);
        self.frames.send(frame).is_ok()
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
        if !outbox.stream(Message::Feed(feed)) {
            return;
        }
    }
}

fn write_all(stream: impl Stream, receiver: &Receiver<Frame>, pending: &Pending) {
    let mut out = BufWriter::new(stream);
    for frame in receiver {
        let written = write_frame(&mut out, &frame)
            .map_err(|error| error.to_string())
            .and_then(|()| out.flush().map_err(|error| error.to_string()));
        if let Err(error) = written {
            tracing::debug!(%error, "a runtime connection stopped taking frames");
            break;
        }
        pending.wrote_one();
    }
    pending.close(pending.backlog());
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
