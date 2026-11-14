use futures::executor::block_on;
use futures::StreamExt as _;
use krad_client::{Runner, RuntimeClient};
use krad_domain::{AgentPreset, GroupId, HostedOrigin, ImageFormat, LaunchIntent, TerminalId};
use krad_protocol::{
    AgentRef, Answer, ChatCommand, CloudCommand, Command, EventKind, Feed, HostedSession,
    HostedWorkspace, PastedImage, Problem, ProblemScope, Reply, Scope, Seq, TerminalCommand,
    WriteBy,
};
use krad_runtime::RuntimeHandle;
use std::error::Error;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::cloud::{environment, TestResult, RUNNER};
use super::mirror::ChatFeed;
use super::scenarios::{sent, settled_with, PROMPT};

const WAIT: Duration = Duration::from_secs(30);
const WATCHED_TERMINAL: TerminalId = TerminalId(900);
const ANSWERED_BEFORE: usize = 1;
const ECHOED: &str = "krad-watched";
const BROKEN_SLUG: &str = "broken-session";
const BROKEN_REMOTE: &str = "/nonexistent/krad-origin.git";
const NOT_SUMMARISED: &str = "the runner's snapshot has no hosted session for the chat";
const NO_BYTES: &str = "the watched terminal never showed the echo";
const NO_PROBLEM: &str = "the runner never reported the broken cloud session";
const RESUMED_WITH_SNAPSHOT: &str = "a resumed client was sent a snapshot";
const TIMED_OUT: &str = "the awaited feed never came";
const PASTED: &[u8] = b"\x89PNG-krad";

fn runner(runtime: &RuntimeHandle) -> Result<Runner, Box<dyn Error>> {
    let line = block_on(runtime.connect_runner(RUNNER))??;
    Ok(Runner::connect(line.input, line.output, line.hello)?)
}

fn feeds(client: &Runner, since: Option<Seq>) -> Receiver<Feed> {
    let mut stream = client.subscribe(since);
    let (sender, received) = channel();
    std::thread::spawn(move || {
        while let Some(feed) = block_on(stream.next()) {
            if sender.send(feed).is_err() {
                break;
            }
        }
    });
    received
}

fn next_feed(received: &Receiver<Feed>, deadline: Instant) -> Result<Feed, Box<dyn Error>> {
    let left = deadline.saturating_duration_since(Instant::now());
    Ok(received.recv_timeout(left).map_err(|_| TIMED_OUT)?)
}

fn metadata_names_the_origin(client: &Runner, id: &AgentRef, group: GroupId) -> TestResult {
    let snapshot = block_on(client.snapshot(Scope::Runtime))?;
    let summary = snapshot
        .hosted
        .iter()
        .find(|summary| {
            summary
                .chats
                .iter()
                .any(|chat| chat.target.terminal == id.terminal)
        })
        .ok_or(NOT_SUMMARISED)?;
    let origin = HostedOrigin {
        environment: environment(),
        group,
    };
    assert_eq!(summary.origin, Some(origin));
    assert!(summary.branch.starts_with("krad/"), "{}", summary.branch);
    let chat = summary.chats.first().ok_or(NOT_SUMMARISED)?;
    assert_eq!(chat.target.harness, Some(AgentPreset::ClaudeCode));
    assert_eq!(chat.target.environment, environment());
    assert!(chat.seq.is_some());
    Ok(())
}

fn resumes_from_its_last_seq(runtime: &RuntimeHandle, id: &AgentRef) -> TestResult {
    let deadline = Instant::now() + WAIT;
    let first = runner(runtime)?;
    let heard = feeds(&first, None);
    let Feed::Snapshot(snapshot) = next_feed(&heard, deadline)? else {
        return Err(TIMED_OUT.into());
    };
    drop((heard, first));
    let driver: Arc<dyn RuntimeClient> = Arc::new(runner(runtime)?);
    let mut chats = ChatFeed::follow_client(Arc::clone(&driver));
    chats.until_chat(id, |mirror| settled_with(mirror, ANSWERED_BEFORE))?;
    assert_eq!(block_on(driver.send(sent(id, PROMPT)))?, Reply::Accepted);
    chats.until_chat(id, |mirror| settled_with(mirror, ANSWERED_BEFORE + 1))?;
    let back = runner(runtime)?;
    let resumed = feeds(&back, Some(snapshot.seq));
    match next_feed(&resumed, deadline)? {
        Feed::Event(event) => assert!(event.seq > snapshot.seq),
        Feed::Snapshot(_) => return Err(RESUMED_WITH_SNAPSHOT.into()),
    }
    Ok(())
}

fn terminal_frames_cross_the_connection(client: &Runner, environment: &str) -> TestResult {
    let terminal = AgentRef {
        environment: krad_domain::EnvironmentName::new(environment.to_owned()),
        terminal: WATCHED_TERMINAL,
    };
    let open = TerminalCommand::Open {
        terminal: terminal.clone(),
        intent: LaunchIntent::Preset {
            preset: AgentPreset::Shell,
        },
        cwd: None,
    };
    assert_eq!(
        block_on(client.send(Command::Terminal(open)))?,
        Reply::Accepted
    );
    let deadline = Instant::now() + WAIT;
    let (reply, mut bytes) = client.watch_terminal(terminal.clone());
    assert_eq!(block_on(reply)?, Reply::Accepted);
    let write = TerminalCommand::Write {
        terminal: terminal.clone(),
        bytes: format!("echo {ECHOED}\n").into_bytes(),
        by: WriteBy::User,
    };
    assert_eq!(
        block_on(client.send(Command::Terminal(write)))?,
        Reply::Accepted
    );
    let (seen, reading) = channel();
    std::thread::spawn(move || {
        let mut shown = Vec::new();
        while let Some(chunk) = block_on(bytes.next()) {
            shown.extend(chunk);
            if String::from_utf8_lossy(&shown).matches(ECHOED).count() >= 2 {
                seen.send(()).unwrap_or_default();
                return;
            }
        }
    });
    reading
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|_| NO_BYTES)?;
    let close = TerminalCommand::Close { terminal };
    assert_eq!(
        block_on(client.send(Command::Terminal(close)))?,
        Reply::Accepted
    );
    Ok(())
}

fn a_broken_clone_is_reported_for_its_slug(client: &Runner) -> TestResult {
    let heard = feeds(client, None);
    let session = Box::new(HostedSession {
        name: "Broken".to_owned(),
        workspace: HostedWorkspace {
            remote_url: BROKEN_REMOTE.to_owned(),
            upstream_branch: None,
            session_slug: BROKEN_SLUG.to_owned(),
            patch: None,
        },
        starts: Vec::new(),
        origin: None,
    });
    let host = Command::Cloud(CloudCommand::Host { session });
    assert_eq!(block_on(client.send(host))?, Reply::Accepted);
    let deadline = Instant::now() + WAIT;
    loop {
        let Feed::Event(event) = next_feed(&heard, deadline).map_err(|_| NO_PROBLEM)? else {
            continue;
        };
        if let EventKind::Problem(Problem {
            scope: ProblemScope::CloudSession { slug },
            ..
        }) = event.kind
        {
            assert_eq!(slug, BROKEN_SLUG);
            return Ok(());
        }
    }
}

fn a_pasted_image_is_read_back_as_bytes(client: &Runner, id: &AgentRef) -> TestResult {
    let heard = feeds(client, None);
    let attach = Command::Chat(ChatCommand::AttachImage {
        chat: id.clone(),
        image: PastedImage {
            format: ImageFormat::Png,
            bytes: PASTED.to_vec(),
        },
    });
    assert_eq!(block_on(client.send(attach))?, Reply::Accepted);
    let deadline = Instant::now() + WAIT;
    let image = loop {
        if let Feed::Event(event) = next_feed(&heard, deadline)? {
            if let EventKind::ChatImageAttached { image, .. } = event.kind {
                break image;
            }
        }
    };
    let request = client.next_request();
    let read = Command::Chat(ChatCommand::ReadImage { image });
    assert_eq!(block_on(client.send_as(request, read))?, Reply::Accepted);
    loop {
        if let Feed::Event(event) = next_feed(&heard, deadline)? {
            if let EventKind::Answered {
                request: answered,
                answer,
            } = event.kind
            {
                if answered == request {
                    let bytes = Answer::ImageBytes {
                        format: ImageFormat::Png,
                        bytes: PASTED.to_vec(),
                    };
                    assert_eq!(*answer, bytes);
                    return Ok(());
                }
            }
        }
    }
}

pub fn the_runner_shows_its_hosted_sessions_terminals_and_failures(
    runtime: &RuntimeHandle,
    id: &AgentRef,
    group: GroupId,
) -> TestResult {
    let client = runner(runtime)?;
    metadata_names_the_origin(&client, id, group)?;
    terminal_frames_cross_the_connection(&client, id.environment.as_str())?;
    a_pasted_image_is_read_back_as_bytes(&client, id)?;
    a_broken_clone_is_reported_for_its_slug(&client)?;
    resumes_from_its_last_seq(runtime, id)
}
