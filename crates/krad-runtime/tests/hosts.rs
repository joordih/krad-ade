#![cfg(unix)]

use futures::executor::block_on;
use futures::StreamExt as _;
use krad_domain::{
    EnvironmentData, EnvironmentName, Host, ProcessRun, PromptAsker, RuntimeState, SshDestination,
    TerminalId,
};
use krad_host::mux::SignInPrompt;
use krad_protocol::{
    AgentRef, Command, EnvironmentCommand, ErrorCode, EventKind, Feed, HostCommand, HostState,
    PromptAnswer, Reply, Scope, TerminalSummary,
};
use krad_runtime::{EnvironmentsSetup, Runtime, RuntimeHandle, RuntimeSetup};
use std::error::Error;
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::sync::Once;
use std::time::{Duration, Instant};

type TestResult = Result<(), Box<dyn Error>>;

const WAIT: Duration = Duration::from_secs(20);
const POLL: Duration = Duration::from_millis(50);
const TIMED_OUT: &str = "the awaited event never came";
const FEED_ENDED: &str = "the feed ended";
const NOT_A_SNAPSHOT: &str = "the feed did not start from a snapshot";
const FAKE_SSH_DIR: &str = "tests/fixtures/fake-ssh";
const PATH_VAR: &str = "PATH";
const REFUSES: &str = "refuses.invalid";
const ASKS: &str = "asks.invalid";
const DECLINES: &str = "declines.invalid";
const SIGN_IN_TERMINAL: TerminalId = TerminalId(50);

static FAKE_SSH: Once = Once::new();

fn use_fake_ssh() {
    FAKE_SSH.call_once(|| {
        let fake = Path::new(env!("CARGO_MANIFEST_DIR")).join(FAKE_SSH_DIR);
        let mut search = vec![fake];
        search.extend(
            std::env::var_os(PATH_VAR)
                .iter()
                .flat_map(std::env::split_paths),
        );
        if let Ok(path) = std::env::join_paths(search) {
            std::env::set_var(PATH_VAR, path);
        }
    });
}

fn named(name: &str) -> EnvironmentName {
    EnvironmentName::new(name.to_owned())
}

fn on(name: &str, host: Option<&str>) -> Result<EnvironmentData, Box<dyn Error>> {
    let mut data = EnvironmentData::empty(named(name));
    if let Some(host) = host {
        data.host = Host::Ssh {
            destination: host.parse::<SshDestination>()?,
        };
    }
    Ok(data)
}

fn runtime_with(environments: Vec<EnvironmentData>) -> Result<RuntimeHandle, Box<dyn Error>> {
    use_fake_ssh();
    let state = RuntimeState {
        environments,
        ..RuntimeState::default()
    };
    Ok(Runtime::start(RuntimeSetup {
        environments: EnvironmentsSetup { state, file: None },
        ..RuntimeSetup::default()
    })?)
}

fn follow(runtime: &RuntimeHandle) -> Result<Receiver<Feed>, Box<dyn Error>> {
    let mut feed = runtime.subscribe();
    let (heard, received) = channel();
    std::thread::spawn(move || {
        while let Some(next) = block_on(feed.next()) {
            if heard.send(next).is_err() {
                break;
            }
        }
    });
    match received.recv_timeout(WAIT)? {
        Feed::Snapshot(_) => Ok(received),
        Feed::Event(_) => Err(NOT_A_SNAPSHOT.into()),
    }
}

fn until(
    feed: &Receiver<Feed>,
    mut wanted: impl FnMut(&EventKind) -> bool,
) -> Result<EventKind, Box<dyn Error>> {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        match feed.recv_timeout(POLL) {
            Ok(Feed::Event(event)) if wanted(&event.kind) => return Ok(event.kind),
            Ok(_) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Err(FEED_ENDED.into()),
        }
    }
    Err(TIMED_OUT.into())
}

fn host_state(feed: &Receiver<Feed>, environment: &str) -> Result<HostState, Box<dyn Error>> {
    match until(
        feed,
        |kind| matches!(kind, EventKind::HostChanged { environment: changed, .. } if changed.as_str() == environment),
    )? {
        EventKind::HostChanged { state, .. } => Ok(state),
        _ => Err(TIMED_OUT.into()),
    }
}

fn host(command: HostCommand) -> Command {
    Command::Host(command)
}

fn show(environment: &str) -> Command {
    host(HostCommand::Show {
        environment: named(environment),
    })
}

fn failure_of(state: &HostState) -> Option<&str> {
    match state {
        HostState::Failed { reason } => Some(reason),
        _ => None,
    }
}

#[test]
fn a_refused_destination_fails_with_its_explanation_and_can_be_reconnected() -> TestResult {
    let runtime = runtime_with(vec![on("box", Some(REFUSES))?])?;
    let feed = follow(&runtime)?;
    assert_eq!(
        block_on(runtime.send(runtime.next_request(), show("box")))?,
        Reply::Accepted
    );
    assert_eq!(host_state(&feed, "box")?, HostState::Connecting);
    let failed = host_state(&feed, "box")?;
    assert!(failure_of(&failed).is_some_and(|reason| reason.contains("refused the connection")));
    let snapshot = block_on(runtime.snapshot(Scope::Runtime))?;
    assert_eq!(snapshot.hosts.hosts.len(), 1);
    assert_eq!(snapshot.hosts.hosts[0].state, failed);
    runtime.post(show("box"));
    runtime.post(host(HostCommand::Reconnect {
        environment: named("box"),
    }));
    assert_eq!(host_state(&feed, "box")?, HostState::Connecting);
    assert!(failure_of(&host_state(&feed, "box")?).is_some());
    Ok(())
}

#[test]
fn a_host_asking_for_a_password_signs_in_through_a_runtime_terminal() -> TestResult {
    let runtime = runtime_with(vec![on("box", Some(ASKS))?])?;
    let feed = follow(&runtime)?;
    runtime.post(show("box"));
    until(
        &feed,
        |kind| matches!(kind, EventKind::SignInWanted { environment } if environment.as_str() == "box"),
    )?;
    let sign_in = AgentRef {
        environment: named("box"),
        terminal: SIGN_IN_TERMINAL,
    };
    let view = runtime.open_sign_in(sign_in);
    assert_eq!(
        host_state(&feed, "box")?,
        HostState::SigningIn {
            terminal: SIGN_IN_TERMINAL
        }
    );
    assert_eq!(host_state(&feed, "box")?, HostState::SignedIn);
    assert_eq!(view.status().run, ProcessRun::Exited { code: Some(0) });
    assert_eq!(host_state(&feed, "box")?, HostState::Connecting);
    let kept = host_state(&feed, "box")?;
    assert!(failure_of(&kept).is_some_and(|reason| reason.contains("did not stay open")));
    let snapshot = block_on(runtime.snapshot(Scope::Runtime))?;
    assert!(snapshot
        .terminals
        .iter()
        .all(|summary| summary.terminal.terminal != SIGN_IN_TERMINAL));
    Ok(())
}

#[test]
fn a_declined_sign_in_fails_with_the_exit_code_and_keeps_its_terminal() -> TestResult {
    let runtime = runtime_with(vec![on("box", Some(DECLINES))?])?;
    let feed = follow(&runtime)?;
    runtime.post(show("box"));
    until(&feed, |kind| matches!(kind, EventKind::SignInWanted { .. }))?;
    drop(runtime.open_sign_in(AgentRef {
        environment: named("box"),
        terminal: SIGN_IN_TERMINAL,
    }));
    host_state(&feed, "box")?;
    let declined = host_state(&feed, "box")?;
    assert!(failure_of(&declined).is_some_and(|reason| reason.contains("code 255")));
    let snapshot = block_on(runtime.snapshot(Scope::Runtime))?;
    assert!(snapshot
        .terminals
        .iter()
        .any(|summary| summary.terminal.terminal == SIGN_IN_TERMINAL));
    Ok(())
}

#[test]
fn a_sign_in_terminal_nobody_waits_for_fails_at_once() -> TestResult {
    let runtime = runtime_with(vec![on("box", Some(ASKS))?])?;
    let feed = follow(&runtime)?;
    let stray = AgentRef {
        environment: named("box"),
        terminal: SIGN_IN_TERMINAL,
    };
    let view = runtime.open_sign_in(stray.clone());
    let ended = until(&feed, |kind| matches!(kind, EventKind::TerminalEnded(_)))?;
    let EventKind::TerminalEnded(TerminalSummary { terminal, run }) = ended else {
        return Err(TIMED_OUT.into());
    };
    assert_eq!(terminal, stray);
    assert!(matches!(run, ProcessRun::Failed { .. }));
    assert!(matches!(view.status().run, ProcessRun::Failed { .. }));
    Ok(())
}

#[test]
fn a_sign_in_prompt_waits_for_its_answer_and_is_answered_once() -> TestResult {
    let runtime = runtime_with(Vec::new())?;
    let feed = follow(&runtime)?;
    let prompts = runtime.prompt_sender().ok_or("the runtime takes prompts")?;
    let (answer, answered) = channel();
    prompts.send(SignInPrompt::new(
        PromptAsker::Git,
        "Password: ".to_owned(),
        answer,
    ))?;
    let EventKind::HostPrompted(prompt) =
        until(&feed, |kind| matches!(kind, EventKind::HostPrompted(_)))?
    else {
        return Err(TIMED_OUT.into());
    };
    assert_eq!(prompt.asker, PromptAsker::Git);
    assert_eq!(prompt.text, "Password: ");
    let snapshot = block_on(runtime.snapshot(Scope::Runtime))?;
    assert_eq!(snapshot.hosts.prompts, vec![prompt.clone()]);
    let reply = block_on(runtime.send(
        runtime.next_request(),
        host(HostCommand::AnswerPrompt {
            prompt: prompt.prompt,
            answer: Some(PromptAnswer::new("hunter2".to_owned())),
        }),
    ))?;
    assert_eq!(reply, Reply::Accepted);
    let secret = answered
        .recv_timeout(WAIT)?
        .ok_or("the prompt got an answer")?;
    assert_eq!(secret.expose(), "hunter2");
    until(
        &feed,
        |kind| matches!(kind, EventKind::HostPromptSettled { prompt: settled } if *settled == prompt.prompt),
    )?;
    let again = block_on(runtime.send(
        runtime.next_request(),
        host(HostCommand::AnswerPrompt {
            prompt: prompt.prompt,
            answer: None,
        }),
    ))?;
    assert!(matches!(
        again,
        Reply::Refused {
            code: ErrorCode::NotFound,
            ..
        }
    ));
    Ok(())
}

#[test]
fn a_local_environment_never_connects_and_an_unknown_one_is_refused() -> TestResult {
    let runtime = runtime_with(vec![on("here", None)?])?;
    assert_eq!(
        block_on(runtime.send(runtime.next_request(), show("here")))?,
        Reply::Accepted
    );
    let unknown = block_on(runtime.send(runtime.next_request(), show("nowhere")))?;
    assert!(matches!(
        unknown,
        Reply::Refused {
            code: ErrorCode::NotFound,
            ..
        }
    ));
    let snapshot = block_on(runtime.snapshot(Scope::Runtime))?;
    assert_eq!(snapshot.hosts.hosts, Vec::new());
    Ok(())
}

#[test]
fn a_closed_environment_leaves_the_hosts() -> TestResult {
    let runtime = runtime_with(vec![on("box", Some(REFUSES))?])?;
    let feed = follow(&runtime)?;
    runtime.post(show("box"));
    host_state(&feed, "box")?;
    host_state(&feed, "box")?;
    let closed = block_on(runtime.send(
        runtime.next_request(),
        Command::Environments(EnvironmentCommand::Close {
            environment: named("box"),
        }),
    ))?;
    assert_eq!(closed, Reply::Accepted);
    let snapshot = block_on(runtime.snapshot(Scope::Runtime))?;
    assert_eq!(snapshot.hosts.hosts, Vec::new());
    Ok(())
}
