use futures::channel::mpsc;
use krad_protocol::{Event, Feed, Snapshot};

#[derive(Debug, Default)]
pub(crate) struct Subscribers(Vec<Subscriber>);

#[derive(Debug)]
struct Subscriber {
    feed: mpsc::Sender<Feed>,
    standing: Standing,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Standing {
    Current,
    Behind,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Delivery {
    Sent,
    Full,
    Gone,
}

impl Subscribers {
    pub(crate) fn add(&mut self, feed: mpsc::Sender<Feed>) {
        self.0.push(Subscriber {
            feed,
            standing: Standing::Behind,
        });
    }

    pub(crate) fn publish(&mut self, events: &[Event], snapshot: impl Fn() -> Snapshot) {
        self.0
            .retain_mut(|subscriber| subscriber.catch_up(events, &snapshot) != Delivery::Gone);
    }
}

impl Subscriber {
    fn catch_up(&mut self, events: &[Event], snapshot: &impl Fn() -> Snapshot) -> Delivery {
        if self.standing == Standing::Behind {
            let delivery = self.offer(Feed::Snapshot(snapshot()));
            if delivery == Delivery::Sent {
                self.standing = Standing::Current;
            }
            return delivery;
        }
        for event in events {
            match self.offer(Feed::Event(event.clone())) {
                Delivery::Sent => {}
                Delivery::Full => {
                    tracing::debug!(
                        seq = event.seq.0,
                        "a slow subscriber falls back to a snapshot"
                    );
                    self.standing = Standing::Behind;
                    return Delivery::Full;
                }
                Delivery::Gone => return Delivery::Gone,
            }
        }
        Delivery::Sent
    }

    fn offer(&mut self, feed: Feed) -> Delivery {
        match self.feed.try_send(feed) {
            Ok(()) => Delivery::Sent,
            Err(error) if error.is_full() => Delivery::Full,
            Err(_) => Delivery::Gone,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_protocol::{EventKind, RequestId, Scope, Seq};

    fn pong(seq: u64) -> Event {
        Event {
            seq: Seq(seq),
            kind: EventKind::Pong {
                request: RequestId(seq),
            },
        }
    }

    fn at(seq: u64) -> Snapshot {
        Snapshot {
            scope: Scope::Runtime,
            seq: Seq(seq),
            bus: krad_protocol::BusSnapshot::default(),
            attention: krad_protocol::AttentionSnapshot::default(),
            health: krad_protocol::HealthSnapshot::default(),
            usage: krad_protocol::UsageSnapshot::default(),
            models: krad_protocol::ModelsSnapshot::default(),
            environments: Vec::new(),
            terminals: Vec::new(),
            hosts: krad_protocol::HostsSnapshot::default(),
            chats: Vec::new(),
            opencode: krad_protocol::OpencodeSnapshot::default(),
            gateway: krad_protocol::GatewayState::default(),
            settings: Box::default(),
            git: Vec::new(),
        }
    }

    fn drain(receiver: &mut mpsc::Receiver<Feed>) -> Vec<Feed> {
        std::iter::from_fn(|| receiver.try_recv().ok()).collect()
    }

    #[test]
    fn a_new_subscriber_starts_from_a_snapshot_then_follows_the_events() {
        let (feed, mut receiver) = mpsc::channel(8);
        let mut subscribers = Subscribers::default();
        subscribers.add(feed);
        subscribers.publish(&[pong(1)], || at(1));
        subscribers.publish(&[pong(2), pong(3)], || at(3));
        assert_eq!(
            drain(&mut receiver),
            vec![
                Feed::Snapshot(at(1)),
                Feed::Event(pong(2)),
                Feed::Event(pong(3)),
            ]
        );
    }

    #[test]
    fn a_full_subscriber_is_marked_behind_and_resyncs_with_a_fresh_snapshot() {
        let (feed, mut receiver) = mpsc::channel(0);
        let mut subscribers = Subscribers::default();
        subscribers.add(feed);
        subscribers.publish(&[], || at(0));
        subscribers.publish(&[pong(1), pong(2)], || at(2));
        assert_eq!(subscribers.0[0].standing, Standing::Behind);
        assert_eq!(drain(&mut receiver), vec![Feed::Snapshot(at(0))]);

        subscribers.publish(&[pong(3)], || at(3));
        assert_eq!(subscribers.0[0].standing, Standing::Current);
        assert_eq!(drain(&mut receiver), vec![Feed::Snapshot(at(3))]);

        subscribers.publish(&[pong(4)], || at(4));
        assert_eq!(drain(&mut receiver), vec![Feed::Event(pong(4))]);
    }

    #[test]
    fn a_slow_subscriber_does_not_hold_back_the_others() {
        let (slow, _unread) = mpsc::channel(0);
        let (fast, mut receiver) = mpsc::channel(8);
        let mut subscribers = Subscribers::default();
        subscribers.add(slow);
        subscribers.add(fast);
        subscribers.publish(&[], || at(0));
        subscribers.publish(&[pong(1), pong(2)], || at(2));
        assert_eq!(
            drain(&mut receiver),
            vec![
                Feed::Snapshot(at(0)),
                Feed::Event(pong(1)),
                Feed::Event(pong(2)),
            ]
        );
    }

    #[test]
    fn a_subscriber_that_left_is_dropped() {
        let (feed, receiver) = mpsc::channel(8);
        let mut subscribers = Subscribers::default();
        subscribers.add(feed);
        drop(receiver);
        subscribers.publish(&[pong(1)], || at(1));
        assert!(subscribers.0.is_empty());
    }
}
