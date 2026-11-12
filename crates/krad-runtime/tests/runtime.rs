use futures::executor::block_on;
use futures::StreamExt as _;
use krad_protocol::{
    AttentionSnapshot, BusSnapshot, Command, ErrorCode, Event, EventKind, Feed, Reply, RequestId,
    RuntimeCommand, Scope, Seq, Snapshot,
};
use krad_runtime::{EventReceiver, Runtime, RuntimeHandle, RuntimeSetup};
use std::error::Error;

type TestResult = Result<(), Box<dyn Error>>;

const FLOOD: u64 = 4096;
const FEED_ENDED: &str = "the feed ended";
const NOT_A_SNAPSHOT: &str = "the feed did not start from a snapshot";

fn ping(runtime: &RuntimeHandle, id: u64) -> Result<Reply, Box<dyn Error>> {
    Ok(block_on(runtime.send(
        RequestId(id),
        Command::Runtime(RuntimeCommand::Ping),
    ))?)
}

fn next(events: &mut EventReceiver) -> Result<Feed, Box<dyn Error>> {
    Ok(block_on(events.next()).ok_or(FEED_ENDED)?)
}

fn start_seq(events: &mut EventReceiver) -> Result<Seq, Box<dyn Error>> {
    match next(events)? {
        Feed::Snapshot(snapshot) => Ok(snapshot.seq),
        Feed::Event(_) => Err(NOT_A_SNAPSHOT.into()),
    }
}

fn pong(seq: u64, request: u64) -> Feed {
    Feed::Event(Event {
        seq: Seq(seq),
        kind: EventKind::Pong {
            request: RequestId(request),
        },
    })
}

#[test]
fn a_known_command_is_accepted_and_an_unknown_one_refused_as_unsupported() -> TestResult {
    let runtime = Runtime::start(RuntimeSetup::default())?;
    assert_eq!(ping(&runtime, 1)?, Reply::Accepted);
    for command in [Command::Unknown, Command::Runtime(RuntimeCommand::Unknown)] {
        let reply = block_on(runtime.send(RequestId(2), command))?;
        assert!(matches!(
            reply,
            Reply::Refused {
                code: ErrorCode::Unsupported,
                ..
            }
        ));
    }
    Ok(())
}

#[test]
fn a_subscriber_starts_from_a_snapshot_and_receives_events_with_rising_seqs() -> TestResult {
    let runtime = Runtime::start(RuntimeSetup::default())?;
    ping(&runtime, 1)?;
    let mut events = runtime.subscribe();
    assert_eq!(start_seq(&mut events)?, Seq(1));
    for id in 10..13 {
        ping(&runtime, id)?;
    }
    let received = (0..3)
        .map(|_| next(&mut events))
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(received, vec![pong(2, 10), pong(3, 11), pong(4, 12)]);
    assert_eq!(
        block_on(runtime.snapshot(Scope::Runtime))?,
        Snapshot {
            scope: Scope::Runtime,
            seq: Seq(4),
            bus: BusSnapshot::default(),
            attention: AttentionSnapshot::default(),
        }
    );
    Ok(())
}

#[test]
fn every_subscriber_receives_the_same_events() -> TestResult {
    let runtime = Runtime::start(RuntimeSetup::default())?;
    let mut first = runtime.subscribe();
    let mut second = runtime.clone().subscribe();
    for events in [&mut first, &mut second] {
        assert_eq!(start_seq(events)?, Seq::default());
    }
    ping(&runtime, 7)?;
    for events in [&mut first, &mut second] {
        assert_eq!(next(events)?, pong(1, 7));
    }
    Ok(())
}

#[test]
fn a_subscriber_that_falls_behind_resyncs_from_a_fresh_snapshot() -> TestResult {
    let runtime = Runtime::start(RuntimeSetup::default())?;
    let mut events = runtime.subscribe();
    let mut delivered = start_seq(&mut events)?;
    for id in 1..=FLOOD {
        drop(runtime.send(RequestId(id), Command::Runtime(RuntimeCommand::Ping)));
    }
    ping(&runtime, FLOOD + 1)?;
    let last = Seq(FLOOD + 1);
    let resynced = loop {
        match next(&mut events)? {
            Feed::Event(event) => {
                assert_eq!(event.seq, delivered.next());
                delivered = event.seq;
            }
            Feed::Snapshot(snapshot) => break snapshot.seq,
        }
    };
    assert!(delivered < last, "the subscriber was never behind");
    assert_eq!(resynced, last);
    ping(&runtime, FLOOD + 2)?;
    assert_eq!(next(&mut events)?, pong(FLOOD + 2, FLOOD + 2));
    Ok(())
}

#[test]
fn the_runtime_stops_when_every_handle_is_dropped() -> TestResult {
    let runtime = Runtime::start(RuntimeSetup::default())?;
    let copy = runtime.clone();
    let mut events = runtime.subscribe();
    start_seq(&mut events)?;
    drop(runtime);
    ping(&copy, 1)?;
    drop(copy);
    assert_eq!(next(&mut events)?, pong(1, 1));
    assert!(block_on(events.next()).is_none());
    Ok(())
}
