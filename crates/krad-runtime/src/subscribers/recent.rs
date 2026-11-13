use krad_protocol::{Event, Seq};
use std::collections::VecDeque;

pub(crate) const RECENT_EVENTS: usize = 2048;

#[derive(Debug)]
pub(crate) struct Recent {
    events: VecDeque<Event>,
    capacity: usize,
}

impl Default for Recent {
    fn default() -> Self {
        Self::with_capacity(RECENT_EVENTS)
    }
}

impl Recent {
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            events: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub(crate) fn keep(&mut self, events: &[Event]) {
        for event in events {
            if self.events.len() == self.capacity {
                self.events.pop_front();
            }
            if self.capacity > 0 {
                self.events.push_back(event.clone());
            }
        }
    }

    pub(crate) fn after(&self, since: Seq, now: Seq) -> Option<Vec<Event>> {
        if since > now {
            return None;
        }
        if since == now {
            return Some(Vec::new());
        }
        let oldest = self.events.front()?.seq;
        if oldest > since.next() {
            return None;
        }
        Some(
            self.events
                .iter()
                .filter(|event| event.seq > since)
                .cloned()
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_protocol::{EventKind, RequestId};

    fn pong(seq: u64) -> Event {
        Event {
            seq: Seq(seq),
            kind: EventKind::Pong {
                request: RequestId(seq),
            },
        }
    }

    fn seqs(events: &[Event]) -> Vec<u64> {
        events.iter().map(|event| event.seq.0).collect()
    }

    #[test]
    fn a_seq_inside_the_ring_gets_exactly_the_later_events() {
        let mut recent = Recent::with_capacity(4);
        recent.keep(&[pong(1), pong(2), pong(3)]);
        assert_eq!(
            recent.after(Seq(1), Seq(3)).as_deref().map(seqs),
            Some(vec![2, 3])
        );
        assert_eq!(
            recent.after(Seq(0), Seq(3)).as_deref().map(seqs),
            Some(vec![1, 2, 3])
        );
        assert_eq!(recent.after(Seq(3), Seq(3)), Some(Vec::new()));
    }

    #[test]
    fn a_seq_older_than_the_ring_or_from_the_future_gets_nothing() {
        let mut recent = Recent::with_capacity(2);
        recent.keep(&[pong(1), pong(2), pong(3), pong(4)]);
        assert_eq!(
            recent.after(Seq(2), Seq(4)).as_deref().map(seqs),
            Some(vec![3, 4])
        );
        assert_eq!(recent.after(Seq(1), Seq(4)), None);
        assert_eq!(recent.after(Seq(5), Seq(4)), None);
        assert_eq!(Recent::with_capacity(2).after(Seq(0), Seq(1)), None);
    }
}
