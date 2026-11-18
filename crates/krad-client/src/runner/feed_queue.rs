use futures::Stream;
use krad_protocol::Feed;
use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Waker};

#[derive(Default)]
struct Queued {
    feeds: VecDeque<Feed>,
    waker: Option<Waker>,
    ended: bool,
    dropped: bool,
}

#[derive(Default)]
struct Shared(Mutex<Queued>);

impl Shared {
    fn queued(&self) -> MutexGuard<'_, Queued> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub(super) struct FeedSink(Arc<Shared>);

pub(super) struct FeedSource(Arc<Shared>);

pub(super) fn feed_queue() -> (FeedSink, FeedSource) {
    let shared = Arc::new(Shared::default());
    (FeedSink(Arc::clone(&shared)), FeedSource(shared))
}

impl FeedSink {
    pub(super) fn push(&self, feed: Feed) -> bool {
        let mut queued = self.0.queued();
        if queued.dropped {
            return false;
        }
        if matches!(feed, Feed::Snapshot(_)) {
            queued.feeds.clear();
        }
        queued.feeds.push_back(feed);
        let waker = queued.waker.take();
        drop(queued);
        if let Some(waker) = waker {
            waker.wake();
        }
        true
    }
}

impl Drop for FeedSink {
    fn drop(&mut self) {
        let mut queued = self.0.queued();
        queued.ended = true;
        let waker = queued.waker.take();
        drop(queued);
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

impl Drop for FeedSource {
    fn drop(&mut self) {
        let mut queued = self.0.queued();
        queued.dropped = true;
        queued.feeds.clear();
    }
}

impl Stream for FeedSource {
    type Item = Feed;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Feed>> {
        let mut queued = self.0.queued();
        if let Some(feed) = queued.feeds.pop_front() {
            return Poll::Ready(Some(feed));
        }
        if queued.ended {
            return Poll::Ready(None);
        }
        queued.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;
    use futures::StreamExt as _;
    use krad_protocol::{
        AttentionSnapshot, BusSnapshot, Event, EventKind, GatewayState, HealthSnapshot,
        ModelsSnapshot, OpencodeSnapshot, RequestId, Scope, Seq, Snapshot, UsageSnapshot,
    };

    const MANY: u64 = 10_000;

    fn pong(seq: u64) -> Feed {
        Feed::Event(Event {
            seq: Seq(seq),
            kind: EventKind::Pong {
                request: RequestId(seq),
            },
        })
    }

    fn at(seq: u64) -> Feed {
        Feed::Snapshot(Snapshot {
            scope: Scope::Runtime,
            seq: Seq(seq),
            bus: BusSnapshot::default(),
            attention: AttentionSnapshot::default(),
            health: HealthSnapshot::default(),
            usage: UsageSnapshot::default(),
            models: ModelsSnapshot::default(),
            environments: Vec::new(),
            terminals: Vec::new(),
            hosts: Box::default(),
            chats: Vec::new(),
            opencode: OpencodeSnapshot::default(),
            gateway: GatewayState::default(),
            settings: Box::default(),
            git: Vec::new(),
            kraddies: Box::default(),
            runners: Box::default(),
            hosted: Vec::new(),
            buffers: Vec::new(),
            language_servers: Vec::new(),
        })
    }

    #[test]
    fn feeds_queue_without_waiting_for_the_reader_and_end_with_the_sink() {
        let (sink, source) = feed_queue();
        assert!((1..=MANY).all(|seq| sink.push(pong(seq))));
        drop(sink);
        let read: Vec<Feed> = block_on(source.collect());
        assert_eq!(read.len(), usize::try_from(MANY).unwrap_or_default());
        assert_eq!(read.last(), Some(&pong(MANY)));
    }

    #[test]
    fn a_snapshot_replaces_the_events_still_queued_before_it() {
        let (sink, source) = feed_queue();
        sink.push(pong(1));
        sink.push(pong(2));
        sink.push(at(2));
        sink.push(pong(3));
        drop(sink);
        let read: Vec<Feed> = block_on(source.collect());
        assert_eq!(read, vec![at(2), pong(3)]);
    }

    #[test]
    fn a_dropped_source_refuses_more_feeds() {
        let (sink, source) = feed_queue();
        drop(source);
        assert!(!sink.push(pong(1)));
    }
}
