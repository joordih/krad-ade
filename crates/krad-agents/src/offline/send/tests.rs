use super::*;
use crate::webhook::signature::signature_header;
use krad_domain::{
    AgentPreset, AgentState, EnvironmentName, EventKind, GroupId, TerminalId, Webhook,
    WebhookSecret, WebhookTarget,
};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::thread::JoinHandle;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const STOP: &str = r#"{"process":"api-2-a1b2c3","at":"2026-11-12T10:00:00.000Z","request":{"Hook":{"harness":"claude","event":"Stop"}}}"#;
const SECRET: &str = "s3cr3t";

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

fn serve_once(listener: TcpListener) -> JoinHandle<Option<Received>> {
    std::thread::spawn(move || {
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
        stream
            .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .ok()?;
        Some(received)
    })
}

fn runner_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("krad-offline-{name}-{}", std::process::id()))
}

fn write_webhooks(root: &Path, install: &InstallId, url: &str) -> TestResult {
    let file = RunnerWebhooks {
        hooks: vec![SyncedWebhook {
            hook: Webhook {
                id: WebhookId(7),
                name: "ci".parse()?,
                url: url.parse()?,
                kinds: [EventKind::AgentFinished].into_iter().collect(),
                signing: WebhookSigning::Signed,
            },
            secret: WebhookSecret::parse(SECRET),
        }],
        targets: vec![WebhookTarget {
            process: "api-2-a1b2c3".parse()?,
            environment: EnvironmentName::new("api".to_owned()),
            group: Some(GroupId(3)),
            terminal: TerminalId(2),
            harness: Some(AgentPreset::ClaudeCode),
            state: AgentState::Working,
        }],
    };
    let path = webhooks_file(root, install);
    std::fs::create_dir_all(path.parent().ok_or("no parent")?)?;
    std::fs::write(path, serde_json::to_string(&file)?)?;
    Ok(())
}

fn stop_event() -> Result<QueuedEvent, Box<dyn std::error::Error>> {
    queued_events(STOP).pop().ok_or_else(|| "no event".into())
}

#[test]
fn a_hook_is_signed_and_delivered_and_its_line_marked_for_that_install_only() -> TestResult {
    let root = runner_dir("delivered");
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let url = format!("http://{}/krad", listener.local_addr()?);
    let server = serve_once(listener);
    let (with, without): (InstallId, InstallId) =
        ("0123456789abcdef".parse()?, "fedcba9876543210".parse()?);
    write_webhooks(&root, &with, &url)?;
    let lines = queue_lines(&root, &[with.clone(), without.clone()], &stop_event()?);
    let received = server
        .join()
        .map_err(|_| "server panicked")?
        .ok_or("no request")?;
    let line_of = |install: &InstallId| {
        lines
            .iter()
            .find(|(of, _)| of == install)
            .and_then(|(_, line)| serde_json::from_str::<QueuedEvent>(line).ok())
            .ok_or("no line")
    };
    assert_eq!(
        line_of(&with)?.webhooked,
        [WebhookId(7)].into_iter().collect()
    );
    assert!(line_of(&without)?.webhooked.is_empty());
    let record: EventRecord = serde_json::from_str(&received.body)?;
    assert_eq!(
        record.event,
        krad_domain::AppEvent::AgentFinished {
            terminal: TerminalId(2)
        }
    );
    let sent_at = received.header("x-krad-timestamp").ok_or("no timestamp")?;
    let secret = WebhookSecret::parse(SECRET).ok_or("no secret")?;
    assert_eq!(
        received.header("x-krad-signature-256"),
        Some(signature_header(&secret, sent_at, received.body.as_bytes()).as_str())
    );
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn an_unreachable_hook_leaves_the_line_for_the_app_within_the_wait() -> TestResult {
    let root = runner_dir("unreached");
    let closed = TcpListener::bind("127.0.0.1:0")?.local_addr()?;
    let install: InstallId = "0123456789abcdef".parse()?;
    write_webhooks(&root, &install, &format!("http://{closed}/krad"))?;
    let started = Instant::now();
    let lines = queue_lines(&root, std::slice::from_ref(&install), &stop_event()?);
    assert!(started.elapsed() <= CLI_WAIT);
    let queued: QueuedEvent = serde_json::from_str(&lines[0].1)?;
    assert!(queued.webhooked.is_empty());
    std::fs::remove_dir_all(root)?;
    Ok(())
}
