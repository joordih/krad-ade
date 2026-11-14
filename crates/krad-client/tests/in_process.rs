use futures::executor::block_on;
use futures::StreamExt as _;
use krad_client::{ClientError, Cursor, Feeds, InProcess, RuntimeClient};
use krad_domain::{AttentionKind, AttentionTarget, EnvironmentName, Sight, TerminalId};
use krad_protocol::{
    AttentionCommand, Command, ErrorCode, Event, EventKind, Feed, Hello, ProtocolVersion, Reply,
    Scope,
};
use krad_runtime::{Runtime, RuntimeSetup};
use std::error::Error;

type TestResult = Result<(), Box<dyn Error>>;

const BUILD: &str = env!("CARGO_PKG_VERSION");
const ENVIRONMENT: &str = "main";
const TITLE: &str = "claude";
const FEED_ENDED: &str = "the feed ended";

fn target(terminal: u64) -> AttentionTarget {
    AttentionTarget::new(
        EnvironmentName::new(ENVIRONMENT.to_owned()),
        TerminalId(terminal),
    )
}

fn raise(terminal: u64) -> Command {
    Command::Attention(AttentionCommand::Raise {
        target: target(terminal),
        kind: AttentionKind::Permission,
        sight: Sight::Unwatched,
        title: TITLE.to_owned(),
    })
}

fn next(feeds: &mut Feeds, cursor: &mut Cursor) -> Result<Feed, Box<dyn Error>> {
    let feed = block_on(feeds.next()).ok_or(FEED_ENDED)?;
    cursor.heard(&feed);
    Ok(feed)
}

fn raised(feed: &Feed) -> Option<TerminalId> {
    match feed {
        Feed::Event(Event {
            kind: EventKind::AttentionRaised(notice),
            ..
        }) => Some(notice.target.terminal),
        _ => None,
    }
}

fn until_raised(feeds: &mut Feeds, cursor: &mut Cursor) -> Result<TerminalId, Box<dyn Error>> {
    loop {
        if let Some(terminal) = raised(&next(feeds, cursor)?) {
            return Ok(terminal);
        }
    }
}

#[test]
fn an_in_process_client_greets_sends_follows_and_resumes_where_it_left() -> TestResult {
    let client = InProcess::new(Runtime::start(RuntimeSetup::default())?);
    let mut cursor = Cursor::default();
    let welcome = block_on(client.hello(Hello::new(BUILD)))?;
    let mut feeds = client.subscribe(cursor.welcomed(&welcome));
    assert!(matches!(next(&mut feeds, &mut cursor)?, Feed::Snapshot(_)));
    assert_eq!(block_on(client.send(raise(1)))?, Reply::Accepted);
    assert_eq!(until_raised(&mut feeds, &mut cursor)?, TerminalId(1));
    drop(feeds);
    client.post(raise(2));
    let since = cursor.welcomed(&block_on(client.hello(Hello::new(BUILD)))?);
    assert!(since.is_some());
    let mut back = client.subscribe(since);
    assert_eq!(until_raised(&mut back, &mut cursor)?, TerminalId(2));
    let snapshot = block_on(client.snapshot(Scope::Runtime))?;
    assert_eq!(snapshot.attention.waiting.len(), 2);
    Ok(())
}

#[test]
fn an_in_process_client_of_another_major_version_is_refused() -> TestResult {
    let client = InProcess::new(Runtime::start(RuntimeSetup::default())?);
    let other = Hello {
        protocol: ProtocolVersion(ProtocolVersion::CURRENT.0 + 1),
        build: BUILD.to_owned(),
        pairing: None,
    };
    assert!(matches!(
        block_on(client.hello(other)),
        Err(ClientError::Refused {
            code: ErrorCode::Unsupported,
            ..
        })
    ));
    Ok(())
}
