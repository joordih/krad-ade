use futures::executor::block_on;
use futures::StreamExt as _;
use krad_domain::{AttentionKind, AttentionTarget, EnvironmentName, Sight, TerminalId};
use krad_protocol::{
    AttentionCommand, Command, Event, EventKind, Feed, Hello, ProtocolVersion, Reply,
    RuntimeCommand, Scope, Seq,
};
use krad_runtime::{EventReceiver, Runtime, RuntimeHandle, RuntimeSetup};
use std::collections::HashSet;
use std::error::Error;

type TestResult = Result<(), Box<dyn Error>>;

const ENVIRONMENT: &str = "main";
const TITLE: &str = "claude";
const FEED_ENDED: &str = "the feed ended";
const BEYOND_THE_RING: u64 = 2100;

fn target(terminal: u64) -> AttentionTarget {
    AttentionTarget::new(
        EnvironmentName::new(ENVIRONMENT.to_owned()),
        TerminalId(terminal),
    )
}

fn raise(runtime: &RuntimeHandle, terminal: u64) {
    runtime.post(Command::Attention(AttentionCommand::Raise {
        target: target(terminal),
        kind: AttentionKind::Permission,
        sight: Sight::Unwatched,
        title: TITLE.to_owned(),
    }));
}

fn dismiss(runtime: &RuntimeHandle, terminal: u64) {
    runtime.post(Command::Attention(AttentionCommand::Dismiss {
        target: target(terminal),
    }));
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Client {
    seq: Seq,
    waiting: HashSet<AttentionTarget>,
}

impl Client {
    fn apply(&mut self, feed: &Feed) {
        match feed {
            Feed::Snapshot(snapshot) => {
                self.seq = snapshot.seq;
                self.waiting = snapshot
                    .attention
                    .waiting
                    .iter()
                    .map(|waiting| waiting.target.clone())
                    .collect();
            }
            Feed::Event(event) => {
                self.seq = event.seq;
                match &event.kind {
                    EventKind::AttentionRaised(notice) => {
                        self.waiting.insert(notice.target.clone());
                    }
                    EventKind::AttentionLeft { target } => {
                        self.waiting.remove(target);
                    }
                    _ => {}
                }
            }
        }
    }
}

fn until_pong(
    runtime: &RuntimeHandle,
    feed: &mut EventReceiver,
    client: &mut Client,
) -> Result<Vec<Feed>, Box<dyn Error>> {
    let request = runtime.next_request();
    runtime.post_as(request, Command::Runtime(RuntimeCommand::Ping));
    let mut heard = Vec::new();
    loop {
        let next = block_on(feed.next()).ok_or(FEED_ENDED)?;
        client.apply(&next);
        let done = matches!(&next, Feed::Event(Event { kind: EventKind::Pong { request: pong }, .. }) if *pong == request);
        heard.push(next);
        if done {
            return Ok(heard);
        }
    }
}

fn first(feed: &mut EventReceiver, client: &mut Client) -> Result<Feed, Box<dyn Error>> {
    let next = block_on(feed.next()).ok_or(FEED_ENDED)?;
    client.apply(&next);
    Ok(next)
}

fn subscribed(runtime: &RuntimeHandle) -> Result<(EventReceiver, Client), Box<dyn Error>> {
    let mut feed = runtime.subscribe();
    let mut client = Client::default();
    first(&mut feed, &mut client)?;
    Ok((feed, client))
}

fn events(feeds: &[Feed]) -> Vec<&Event> {
    feeds
        .iter()
        .filter_map(|feed| match feed {
            Feed::Event(event) => Some(event),
            Feed::Snapshot(_) => None,
        })
        .collect()
}

fn the_runtime_state(runtime: &RuntimeHandle) -> Result<Client, Box<dyn Error>> {
    let mut client = Client::default();
    client.apply(&Feed::Snapshot(block_on(runtime.snapshot(Scope::Runtime))?));
    Ok(client)
}

#[test]
fn a_client_resuming_inside_the_ring_gets_exactly_the_events_it_missed() -> TestResult {
    let runtime = Runtime::start(RuntimeSetup::default())?;
    let (mut watching, mut witness) = subscribed(&runtime)?;
    let (mut left, mut leaving) = subscribed(&runtime)?;
    raise(&runtime, 1);
    raise(&runtime, 2);
    until_pong(&runtime, &mut left, &mut leaving)?;
    let mut seen = until_pong(&runtime, &mut watching, &mut witness)?;
    drop(left);
    let since = leaving.seq;
    dismiss(&runtime, 1);
    raise(&runtime, 3);
    seen.extend(until_pong(&runtime, &mut watching, &mut witness)?);
    let mut back = runtime.resume(since);
    let mut resumed = vec![first(&mut back, &mut leaving)?];
    resumed.extend(until_pong(&runtime, &mut back, &mut leaving)?);
    assert!(resumed.iter().all(|feed| matches!(feed, Feed::Event(_))));
    let missed: Vec<&Event> = events(&seen)
        .into_iter()
        .filter(|event| event.seq > since)
        .collect();
    let caught = events(&resumed);
    assert_eq!(caught[..missed.len()], missed[..]);
    assert_eq!(caught.first().map(|event| event.seq), Some(since.next()));
    assert_eq!(leaving.waiting, witness.waiting);
    assert_eq!(leaving.waiting, the_runtime_state(&runtime)?.waiting);
    Ok(())
}

#[test]
fn a_client_resuming_from_before_the_ring_starts_again_from_a_snapshot() -> TestResult {
    let runtime = Runtime::start(RuntimeSetup::default())?;
    let (mut left, mut leaving) = subscribed(&runtime)?;
    raise(&runtime, 1);
    raise(&runtime, 2);
    until_pong(&runtime, &mut left, &mut leaving)?;
    drop(left);
    let since = leaving.seq;
    dismiss(&runtime, 2);
    for _ in 0..BEYOND_THE_RING {
        runtime.post(Command::Runtime(RuntimeCommand::Ping));
    }
    raise(&runtime, 3);
    while block_on(runtime.snapshot(Scope::Runtime))?.seq.0 <= since.0 + BEYOND_THE_RING {
        std::thread::yield_now();
    }
    let mut back = runtime.resume(since);
    let mut resumed = vec![first(&mut back, &mut leaving)?];
    resumed.extend(until_pong(&runtime, &mut back, &mut leaving)?);
    assert!(matches!(resumed.first(), Some(Feed::Snapshot(_))));
    assert_eq!(leaving.waiting, the_runtime_state(&runtime)?.waiting);
    assert_eq!(
        leaving.waiting,
        [target(1), target(3)].into_iter().collect::<HashSet<_>>()
    );
    Ok(())
}

#[test]
fn a_client_from_another_run_cannot_mistake_its_seq_for_this_run() -> TestResult {
    let first = Runtime::start(RuntimeSetup::default())?;
    let second = Runtime::start(RuntimeSetup::default())?;
    let hello = Hello::new(env!("CARGO_PKG_VERSION"));
    let (Ok(one), Ok(other)) = (first.hello(&hello), second.hello(&hello)) else {
        return Err("a runtime refused a client of its own version".into());
    };
    assert_eq!(one.protocol, ProtocolVersion::CURRENT);
    assert_ne!(one.run, other.run);
    let older = Hello {
        protocol: ProtocolVersion(0),
        ..hello
    };
    assert!(matches!(first.hello(&older), Err(Reply::Refused { .. })));
    Ok(())
}
