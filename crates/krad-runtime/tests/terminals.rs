#![cfg(unix)]

use futures::executor::block_on;
use futures::StreamExt as _;
use krad_domain::{
    AgentPreset, AgentState, EnvironmentChange, EnvironmentData, EnvironmentName, GridSize,
    NodeEdit, ProcessRun, RuntimeState, TerminalId, TerminalMeta,
};
use krad_protocol::{
    AgentRef, Command, EnvironmentCommand, ErrorCode, EventKind, Feed, NodeCommand, Reply, Scope,
    TerminalClassified, TerminalCommand, TerminalSummary, WriteBy,
};
use krad_runtime::{
    EnvironmentsSetup, Runtime, RuntimeHandle, RuntimeSetup, TerminalLaunch, TerminalPlace,
    TerminalView,
};
use krad_session::PtyError;
use std::error::Error;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

type TestResult = Result<(), Box<dyn Error>>;

const SHELL: &str = "/bin/sh";
const WAIT: Duration = Duration::from_secs(10);
const TIMED_OUT: &str = "the awaited event never came";
const FEED_ENDED: &str = "the feed ended";
const NOT_A_SNAPSHOT: &str = "the feed did not start from a snapshot";
const POLL: Duration = Duration::from_millis(50);

struct Folder(PathBuf);

impl Folder {
    fn new(name: &str) -> Result<Self, Box<dyn Error>> {
        let dir = std::env::temp_dir().join(format!(
            "krad-runtime-terminals-{name}-{}",
            std::process::id()
        ));
        drop(std::fs::remove_dir_all(&dir));
        std::fs::create_dir_all(&dir)?;
        Ok(Self(dir))
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.0));
    }
}

fn terminal(id: u64) -> AgentRef {
    AgentRef {
        environment: EnvironmentName::new("main".to_owned()),
        terminal: TerminalId(id),
    }
}

fn shell(harness: Option<AgentPreset>) -> TerminalLaunch {
    TerminalLaunch {
        program: SHELL.to_owned(),
        args: Vec::new(),
        cwd: None,
        vars: vec![("PS1".to_owned(), "$ ".to_owned())],
        place: TerminalPlace::Here,
        harness,
        prefill: Vec::new(),
        typed: None,
        tap: None,
    }
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
    view: Option<&TerminalView>,
    mut wanted: impl FnMut(&EventKind) -> bool,
) -> Result<EventKind, Box<dyn Error>> {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if let Some(view) = view {
            drop(view.paint_frame());
        }
        match feed.recv_timeout(POLL) {
            Ok(Feed::Event(event)) if wanted(&event.kind) => return Ok(event.kind),
            Ok(_) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Err(FEED_ENDED.into()),
        }
    }
    Err(TIMED_OUT.into())
}

fn screen_text(view: &TerminalView) -> String {
    view.frame()
        .screen
        .rows
        .iter()
        .map(|row| row.spans.iter().map(|span| span.text.as_str()).collect())
        .collect::<Vec<String>>()
        .join("\n")
}

fn shows(feed: &Receiver<Feed>, view: &TerminalView, text: &str) -> TestResult {
    until(feed, Some(view), |kind| {
        matches!(kind, EventKind::TerminalChanged { .. }) && screen_text(view).contains(text)
    })?;
    Ok(())
}

fn send(runtime: &RuntimeHandle, command: TerminalCommand) -> Result<Reply, Box<dyn Error>> {
    Ok(block_on(
        runtime.send(runtime.next_request(), Command::Terminal(command)),
    )?)
}

#[test]
fn a_shell_in_the_runtime_echoes_input_resizes_and_tells_its_exit() -> TestResult {
    let runtime = Runtime::start(RuntimeSetup::default())?;
    let feed = follow(&runtime)?;
    let mut launch = shell(None);
    launch.prefill = vec!["restored line".to_owned()];
    let view = runtime.open_terminal(terminal(1), launch);
    shows(&feed, &view, "restored line")?;
    runtime.write_terminal(
        terminal(1),
        b"echo krad-$((40 + 2))\r".to_vec(),
        WriteBy::User,
    );
    shows(&feed, &view, "krad-42")?;

    let size = GridSize {
        cols: 90,
        rows: 12,
        cell_width_px: 8,
        cell_height_px: 18,
    };
    let resize = TerminalCommand::Resize {
        terminal: terminal(1),
        size,
    };
    assert_eq!(send(&runtime, resize)?, Reply::Accepted);
    until(&feed, Some(&view), |kind| {
        matches!(kind, EventKind::TerminalChanged { .. }) && view.frame().screen.rows.len() == 12
    })?;

    runtime.write_terminal(terminal(1), b"exit 3\r".to_vec(), WriteBy::User);
    let ended = until(&feed, None, |kind| {
        matches!(kind, EventKind::TerminalEnded(_))
    })?;
    let exited = ProcessRun::Exited { code: Some(3) };
    assert_eq!(
        ended,
        EventKind::TerminalEnded(TerminalSummary {
            terminal: terminal(1),
            run: exited.clone(),
        })
    );
    assert_eq!(view.status().run, exited);
    let snapshot = block_on(runtime.snapshot(Scope::Runtime))?;
    assert_eq!(
        snapshot.terminals,
        vec![TerminalSummary {
            terminal: terminal(1),
            run: exited,
        }]
    );
    Ok(())
}

#[test]
fn an_agent_terminal_is_classified_working_then_waiting_on_a_bell() -> TestResult {
    let runtime = Runtime::start(RuntimeSetup::default())?;
    let feed = follow(&runtime)?;
    let view = runtime.open_terminal(terminal(2), shell(Some(AgentPreset::Codex)));
    let classified = |state: AgentState| {
        EventKind::TerminalClassified(TerminalClassified {
            terminal: terminal(2),
            state,
        })
    };
    let working = classified(AgentState::Working);
    until(&feed, Some(&view), |kind| *kind == working)?;
    runtime.write_terminal(terminal(2), b"printf '\\007'\r".to_vec(), WriteBy::User);
    let waiting = classified(AgentState::WaitingForInput);
    until(&feed, Some(&view), |kind| *kind == waiting)?;
    Ok(())
}

#[test]
fn a_reopened_terminal_ignores_the_exit_of_the_process_it_replaced() -> TestResult {
    let runtime = Runtime::start(RuntimeSetup::default())?;
    let feed = follow(&runtime)?;
    let first = runtime.open_terminal(terminal(3), shell(None));
    shows(&feed, &first, "$")?;
    let second = runtime.open_terminal(terminal(3), shell(None));
    runtime.write_terminal(
        terminal(3),
        b"echo second-$((1 + 1))\r".to_vec(),
        WriteBy::User,
    );
    shows(&feed, &second, "second-2")?;
    assert!(second.is_running());
    let close = TerminalCommand::Close {
        terminal: terminal(3),
    };
    assert_eq!(send(&runtime, close.clone())?, Reply::Accepted);
    assert!(matches!(
        send(&runtime, close)?,
        Reply::Refused {
            code: ErrorCode::NotFound,
            ..
        }
    ));
    Ok(())
}

#[test]
fn a_terminal_that_cannot_open_fails_with_its_reason() -> TestResult {
    let runtime = Runtime::start(RuntimeSetup::default())?;
    let feed = follow(&runtime)?;
    let mut launch = shell(None);
    launch.place = TerminalPlace::Opened(Box::new(|_, _| Err(PtyError::Spawn("no runner".into()))));
    let view = runtime.open_terminal(terminal(4), launch);
    let ended = until(&feed, None, |kind| {
        matches!(kind, EventKind::TerminalEnded(_))
    })?;
    let EventKind::TerminalEnded(TerminalSummary {
        run: ProcessRun::Failed { error },
        ..
    }) = ended
    else {
        return Err("the terminal did not fail".into());
    };
    assert!(error.contains("no runner"));
    assert!(!view.is_running());
    Ok(())
}

#[test]
fn the_runtime_owns_terminal_meta_and_ignores_a_client_writing_it() -> TestResult {
    let folder = Folder::new("meta")?;
    let mut main = EnvironmentData::empty(EnvironmentName::new("main".to_owned()));
    main.nodes.push(serde_json::from_value(serde_json::json!({
        "id": 5, "title": "shell", "state": "Idle"
    }))?);
    let runtime = Runtime::start(RuntimeSetup {
        environments: EnvironmentsSetup {
            state: RuntimeState {
                environments: vec![main],
                ..RuntimeState::default()
            },
            file: None,
        },
        ..RuntimeSetup::default()
    })?;
    let feed = follow(&runtime)?;
    let mut launch = shell(None);
    launch.cwd = Some(folder.0.clone());
    let view = runtime.open_terminal(terminal(5), launch);
    shows(&feed, &view, "$")?;
    block_on(runtime.settle())?;
    let cwd = folder.0.to_string_lossy().into_owned();
    let saved = |environments: &[EnvironmentData]| {
        environments
            .first()
            .and_then(|main| main.meta.get(&TerminalId(5)))
            .map(|meta| meta.cwd.clone())
    };
    assert_eq!(
        saved(&block_on(runtime.snapshot(Scope::Runtime))?.environments),
        Some(cwd.clone())
    );
    let forged = EnvironmentCommand::Change {
        environment: EnvironmentName::new("main".to_owned()),
        changes: vec![EnvironmentChange::Meta {
            terminal: TerminalId(5),
            meta: TerminalMeta {
                cwd: "/elsewhere".to_owned(),
                scrollback: Vec::new(),
            },
        }],
    };
    let forged = block_on(runtime.send(runtime.next_request(), Command::Environments(forged)))?;
    assert_eq!(forged, Reply::Accepted);
    assert_eq!(
        saved(&block_on(runtime.snapshot(Scope::Runtime))?.environments),
        Some(cwd)
    );
    let removed = NodeCommand {
        environment: EnvironmentName::new("main".to_owned()),
        edit: NodeEdit::Close {
            terminal: TerminalId(5),
        },
    };
    runtime.post(Command::Node(removed));
    let pruned = until(&feed, None, |kind| {
        matches!(kind, EventKind::EnvironmentChanged { changes, .. }
            if changes.contains(&EnvironmentChange::MetaRemoved { terminal: TerminalId(5) }))
    })?;
    let EventKind::EnvironmentChanged { changes, .. } = pruned else {
        return Err("not an environment change".into());
    };
    assert!(!changes
        .iter()
        .any(|change| matches!(change, EnvironmentChange::Meta { .. })));
    assert_eq!(
        saved(&block_on(runtime.snapshot(Scope::Runtime))?.environments),
        None
    );
    Ok(())
}
