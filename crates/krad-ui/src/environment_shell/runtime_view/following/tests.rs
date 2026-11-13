use futures::executor::block_on;
use krad_protocol::{Event, EventKind, Feed, Seq};

use super::{Batch, Following};
use crate::fake_client::RecordedClient;

fn event(seq: u64) -> Feed {
    Feed::Event(Event {
        seq: Seq(seq),
        kind: EventKind::AttentionHistoryCleared,
    })
}

fn seqs(batch: Batch) -> Option<Vec<u64>> {
    let Batch::Feeds(feeds) = batch else {
        return None;
    };
    let seqs = feeds
        .iter()
        .map(|feed| match feed {
            Feed::Snapshot(snapshot) => snapshot.seq.0,
            Feed::Event(event) => event.seq.0,
        })
        .collect();
    Some(seqs)
}

#[test]
fn a_view_model_reconnects_after_its_feed_ends_and_resumes_from_its_cursor() {
    let client = RecordedClient::new(vec![event(3), event(4), event(5)]);
    let mut following = Following::new(client.clone());
    assert_eq!(seqs(block_on(following.next_batch())), Some(vec![3, 4, 5]));
    assert!(matches!(block_on(following.next_batch()), Batch::Ended));
    client.record(event(6));
    assert_eq!(
        seqs(block_on(following.next_batch())),
        Some(vec![6]),
        "the resumed feed starts after the last event heard"
    );
    assert!(matches!(block_on(following.next_batch()), Batch::Ended));
    client.stop();
    assert!(matches!(block_on(following.next_batch()), Batch::Stopped));
}
