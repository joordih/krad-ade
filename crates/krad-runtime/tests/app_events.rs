use futures::executor::block_on;
use futures::StreamExt as _;
use krad_agents::webhook_signature;
use krad_domain::{
    AppEvent, DeliveryOutcome, EnvironmentName, EventKind as AppEventKind, EventRecord, GroupId,
    TerminalId, Webhook, WebhookId, WebhookSecret, WebhookSettings, WebhookSigning,
};
use krad_protocol::{Command, EventKind, EventsCommand, Feed, Reply, WebhookTest};
use krad_runtime::{EventReceiver, EventsSetup, Runtime, RuntimeHandle, RuntimeSetup};
use krad_store::Secrets;
use std::collections::BTreeSet;
use std::error::Error;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

type TestResult = Result<(), Box<dyn Error>>;

const ENVIRONMENT: &str = "main";
const SECRET: &str = "s3cr3t";
const GROUP: GroupId = GroupId(3);
const HOOK: WebhookId = WebhookId(7);
const JOURNAL_WAIT: Duration = Duration::from_secs(5);
const JOURNAL_POLL: Duration = Duration::from_millis(20);
const FEED_ENDED: &str = "the feed ended";
const NOT_A_SNAPSHOT: &str = "the feed did not start from a snapshot";
const NO_CONTENT: &[u8] =
    b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

struct Received {
    head: Vec<String>,
    body: String,
}

impl Received {
    fn header(&self, name: &str) -> Option<&str> {
        self.head.iter().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name).then(|| value.trim())
        })
    }
}

struct Folder(PathBuf);

impl Folder {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("krad-runtime-events-{name}-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&dir));
        Self(dir)
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.0));
    }
}

fn serve_once(listener: TcpListener) -> JoinHandle<Option<Received>> {
    thread::spawn(move || {
        let (stream, _) = listener.accept().ok()?;
        let mut reader = BufReader::new(stream.try_clone().ok()?);
        let mut head = Vec::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).ok()?;
            let line = line.trim_end().to_owned();
            if line.is_empty() {
                break;
            }
            head.push(line);
        }
        let mut received = Received {
            head,
            body: String::new(),
        };
        let length: usize = received.header("content-length")?.parse().ok()?;
        let mut body = vec![0; length];
        reader.read_exact(&mut body).ok()?;
        received.body = String::from_utf8(body).ok()?;
        let mut stream = stream;
        stream.write_all(NO_CONTENT).ok()?;
        Some(received)
    })
}

fn hook(url: &str, signing: WebhookSigning) -> Result<Webhook, Box<dyn Error>> {
    Ok(Webhook {
        id: HOOK,
        name: "ci".parse()?,
        url: url.parse()?,
        kinds: [AppEventKind::AgentFinished].into_iter().collect(),
        signing,
    })
}

fn with_events(folder: &Folder) -> Result<RuntimeHandle, Box<dyn Error>> {
    let secrets = Secrets::in_memory();
    let secret = WebhookSecret::parse(SECRET).ok_or("the test secret is invalid")?;
    secrets.write_webhook_secret(HOOK, &secret)?;
    Ok(Runtime::start(RuntimeSetup {
        events: Some(EventsSetup {
            journal: Some(folder.0.clone()),
        }),
        secrets,
        ..RuntimeSetup::default()
    })?)
}

fn send(runtime: &RuntimeHandle, command: EventsCommand) -> Result<Reply, Box<dyn Error>> {
    Ok(block_on(
        runtime.send(runtime.next_request(), Command::Events(command)),
    )?)
}

fn use_hook(runtime: &RuntimeHandle, hook: Webhook) -> Result<Reply, Box<dyn Error>> {
    let mut webhooks = WebhookSettings::default();
    webhooks.save(hook);
    send(runtime, EventsCommand::UseWebhooks { webhooks })
}

fn subscribed(runtime: &RuntimeHandle) -> Result<EventReceiver, Box<dyn Error>> {
    let mut events = runtime.subscribe();
    match block_on(events.next()).ok_or(FEED_ENDED)? {
        Feed::Snapshot(_) => Ok(events),
        Feed::Event(_) => Err(NOT_A_SNAPSHOT.into()),
    }
}

fn next_event(events: &mut EventReceiver) -> Result<EventKind, Box<dyn Error>> {
    loop {
        match block_on(events.next()).ok_or(FEED_ENDED)? {
            Feed::Event(event) => return Ok(event.kind),
            Feed::Snapshot(_) => {}
        }
    }
}

fn journal_lines(dir: &Path) -> Result<Vec<EventRecord>, Box<dyn Error>> {
    let file = dir
        .join(ENVIRONMENT)
        .join(format!("group-{}.jsonl", GROUP.as_u64()));
    let deadline = Instant::now() + JOURNAL_WAIT;
    loop {
        if let Ok(text) = std::fs::read_to_string(&file) {
            if !text.is_empty() {
                return Ok(text.lines().filter_map(EventRecord::read).collect());
            }
        }
        if Instant::now() >= deadline {
            return Err("nothing was journaled".into());
        }
        thread::sleep(JOURNAL_POLL);
    }
}

fn finished() -> AppEvent {
    AppEvent::AgentFinished {
        terminal: TerminalId(2),
    }
}

#[test]
fn a_published_event_is_journaled_signed_to_its_webhook_and_heard_by_clients() -> TestResult {
    let folder = Folder::new("publish");
    let runtime = with_events(&folder)?;
    let mut events = subscribed(&runtime)?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let url = format!("http://{}/krad", listener.local_addr()?);
    let server = serve_once(listener);
    assert_eq!(
        use_hook(&runtime, hook(&url, WebhookSigning::Signed)?)?,
        Reply::Accepted
    );
    let published = EventsCommand::Publish {
        environment: EnvironmentName::new(ENVIRONMENT.to_owned()),
        group: Some(GROUP),
        event: finished(),
        delivered: BTreeSet::new(),
    };
    assert_eq!(send(&runtime, published)?, Reply::Accepted);
    let EventKind::AppEvent(heard) = next_event(&mut events)? else {
        return Err("no app event heard".into());
    };
    assert_eq!(heard.event, finished());
    assert_eq!(heard.group, Some(GROUP));
    let heard: EventRecord = serde_json::from_str(&String::try_from(&heard)?)?;
    let received = server
        .join()
        .map_err(|_| "server panicked")?
        .ok_or("no request")?;
    assert!(received.head[0].starts_with("POST /krad "));
    let sent: EventRecord = serde_json::from_str(&received.body)?;
    assert_eq!(sent, heard);
    let sent_at = received.header("x-krad-timestamp").ok_or("no timestamp")?;
    let secret = WebhookSecret::parse(SECRET).ok_or("no secret")?;
    assert_eq!(
        received.header("x-krad-signature-256"),
        Some(webhook_signature(&secret, sent_at, received.body.as_bytes()).as_str())
    );
    assert_eq!(journal_lines(&folder.0)?, vec![heard]);
    Ok(())
}

#[test]
fn a_webhook_test_reports_its_outcome_and_an_unknown_hook_is_refused() -> TestResult {
    let folder = Folder::new("test");
    let runtime = with_events(&folder)?;
    let mut events = subscribed(&runtime)?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let url = format!("http://{}/krad", listener.local_addr()?);
    let server = serve_once(listener);
    use_hook(&runtime, hook(&url, WebhookSigning::Unsigned)?)?;
    let environment = EnvironmentName::new(ENVIRONMENT.to_owned());
    let unknown = EventsCommand::TestWebhook {
        hook: WebhookId(8),
        environment: environment.clone(),
    };
    assert!(matches!(send(&runtime, unknown)?, Reply::Refused { .. }));
    let known = EventsCommand::TestWebhook {
        hook: HOOK,
        environment,
    };
    assert_eq!(send(&runtime, known)?, Reply::Accepted);
    assert_eq!(
        next_event(&mut events)?,
        EventKind::WebhookTested(WebhookTest {
            hook: HOOK,
            outcome: DeliveryOutcome::Delivered,
        })
    );
    let received = server
        .join()
        .map_err(|_| "server panicked")?
        .ok_or("no request")?;
    let sent: EventRecord = serde_json::from_str(&received.body)?;
    assert_eq!(sent.event, AppEvent::Ping);
    assert_eq!(received.header("x-krad-signature-256"), None);
    Ok(())
}
