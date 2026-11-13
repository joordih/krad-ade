#![cfg(unix)]

use futures::executor::block_on;
use futures::StreamExt as _;
use krad_domain::{
    AgentPreset, EnvironmentData, EnvironmentName, Host, LaunchIntent, RuntimeState, Settings,
    SshDestination, TerminalId, TerminalMeta, TerminalNode,
};
use krad_protocol::{
    AgentRef, Command, EventKind, Feed, HostCommand, HostState, LaunchedAs, Reply, TerminalCommand,
    TerminalLaunched,
};
use krad_runtime::{
    EnvironmentsSetup, Runtime, RuntimeHandle, RuntimeSetup, SettingsSetup, TerminalView,
};
use std::error::Error;
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::sync::Once;
use std::time::{Duration, Instant};

type TestResult = Result<(), Box<dyn Error>>;

const WAIT: Duration = Duration::from_secs(20);
const POLL: Duration = Duration::from_millis(50);
const QUIET: Duration = Duration::from_millis(500);
const TIMED_OUT: &str = "the awaited event never came";
const FEED_ENDED: &str = "the feed ended";
const NOT_A_SNAPSHOT: &str = "the feed did not start from a snapshot";
const NO_VIEW: &str = "the launched terminal left no view";
const FAKE_SSH_DIR: &str = "tests/fixtures/fake-ssh";
const PATH_VAR: &str = "PATH";
const SHELL_VAR: &str = "SHELL";
const SHELL: &str = "/bin/sh";
const CONNECTS: &str = "connects.invalid";
const AGENT_LINE: &str = "echo launched-$((40 + 2))";
const AGENT_OUTPUT: &str = "launched-42";
const SCROLLBACK: &str = "kept from before";

static TEST_SHELLS: Once = Once::new();

fn use_test_shells() {
    TEST_SHELLS.call_once(|| {
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
        std::env::set_var(SHELL_VAR, SHELL);
    });
}

fn named(name: &str) -> EnvironmentName {
    EnvironmentName::new(name.to_owned())
}

fn terminal(environment: &str, id: u64) -> AgentRef {
    AgentRef {
        environment: named(environment),
        terminal: TerminalId(id),
    }
}

fn node(id: u64) -> Result<TerminalNode, serde_json::Error> {
    serde_json::from_value(serde_json::json!({"id": id, "title": "shell", "state": "Idle"}))
}

fn environment(
    name: &str,
    host: Option<&str>,
    nodes: &[u64],
) -> Result<EnvironmentData, Box<dyn Error>> {
    let mut data = EnvironmentData::empty(named(name));
    if let Some(host) = host {
        data.host = Host::Ssh {
            destination: host.parse::<SshDestination>()?,
        };
    }
    data.nodes = nodes.iter().map(|id| node(*id)).collect::<Result<_, _>>()?;
    Ok(data)
}

fn start(
    environments: Vec<EnvironmentData>,
    settings: Settings,
) -> Result<RuntimeHandle, Box<dyn Error>> {
    use_test_shells();
    let state = RuntimeState {
        environments,
        ..RuntimeState::default()
    };
    Ok(Runtime::start(RuntimeSetup {
        environments: EnvironmentsSetup { state, file: None },
        settings: SettingsSetup {
            settings,
            file: None,
        },
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

fn launched(feed: &Receiver<Feed>) -> Result<TerminalLaunched, Box<dyn Error>> {
    match until(feed, None, |kind| {
        matches!(kind, EventKind::TerminalLaunched(_))
    })? {
        EventKind::TerminalLaunched(launched) => Ok(launched),
        _ => Err(TIMED_OUT.into()),
    }
}

fn launches_within(feed: &Receiver<Feed>, quiet: Duration) -> Vec<TerminalLaunched> {
    let deadline = Instant::now() + quiet;
    let mut heard = Vec::new();
    while Instant::now() < deadline {
        if let Ok(Feed::Event(event)) = feed.recv_timeout(POLL) {
            if let EventKind::TerminalLaunched(launched) = event.kind {
                heard.push(launched);
            }
        }
    }
    heard
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

fn terminal_command(
    runtime: &RuntimeHandle,
    command: TerminalCommand,
) -> Result<Reply, Box<dyn Error>> {
    Ok(block_on(
        runtime.send(runtime.next_request(), Command::Terminal(command)),
    )?)
}

fn host_connected(feed: &Receiver<Feed>) -> TestResult {
    until(feed, None, |kind| {
        matches!(
            kind,
            EventKind::HostChanged {
                state: HostState::Connected { .. },
                ..
            }
        )
    })?;
    Ok(())
}

#[test]
fn missing_sessions_are_opened_once_with_their_saved_scrollback() -> TestResult {
    let mut data = environment("main", None, &[1, 2])?;
    data.meta.insert(
        TerminalId(1),
        TerminalMeta {
            cwd: String::new(),
            scrollback: vec![SCROLLBACK.to_owned()],
        },
    );
    let runtime = start(vec![data], Settings::default())?;
    let feed = follow(&runtime)?;
    let open_missing = TerminalCommand::OpenMissing {
        environment: named("main"),
    };
    assert_eq!(
        terminal_command(&runtime, open_missing.clone())?,
        Reply::Accepted
    );
    let mut heard = [launched(&feed)?, launched(&feed)?];
    heard.sort_by_key(|launched| launched.terminal.terminal.as_u64());
    assert_eq!(heard[0].terminal, terminal("main", 1));
    assert_eq!(heard[1].terminal, terminal("main", 2));
    assert!(heard
        .iter()
        .all(|launched| launched.launched_as == LaunchedAs::Shell));
    let view = runtime
        .take_launched_view(&terminal("main", 1))
        .ok_or(NO_VIEW)?;
    shows(&feed, &view, SCROLLBACK)?;
    assert_eq!(terminal_command(&runtime, open_missing)?, Reply::Accepted);
    assert_eq!(launches_within(&feed, QUIET), Vec::new());
    Ok(())
}

#[test]
fn an_agent_launch_types_the_line_from_the_settings_it_has_now() -> TestResult {
    let runtime = start(vec![environment("main", None, &[3])?], Settings::default())?;
    let feed = follow(&runtime)?;
    let mut settings = Settings::default();
    settings
        .harness_commands
        .insert(AgentPreset::ClaudeCode, AGENT_LINE.to_owned());
    let save = krad_protocol::SettingsCommand::Save {
        settings: Box::new(settings),
    };
    runtime.post(Command::Settings(save));
    let open = TerminalCommand::Open {
        terminal: terminal("main", 3),
        intent: LaunchIntent::Preset {
            preset: AgentPreset::ClaudeCode,
        },
        cwd: None,
    };
    assert_eq!(terminal_command(&runtime, open)?, Reply::Accepted);
    let heard = launched(&feed)?;
    assert_eq!(heard.harness, Some(AgentPreset::ClaudeCode));
    assert_eq!(heard.launched_as, LaunchedAs::Line);
    let view = runtime
        .take_launched_view(&terminal("main", 3))
        .ok_or(NO_VIEW)?;
    shows(&feed, &view, AGENT_OUTPUT)?;
    Ok(())
}

#[test]
fn a_deferred_launch_is_released_when_the_host_connects() -> TestResult {
    let runtime = start(
        vec![environment("box", Some(CONNECTS), &[4])?],
        Settings::default(),
    )?;
    let feed = follow(&runtime)?;
    let open = TerminalCommand::Open {
        terminal: terminal("box", 4),
        intent: LaunchIntent::Preset {
            preset: AgentPreset::Shell,
        },
        cwd: None,
    };
    assert_eq!(terminal_command(&runtime, open)?, Reply::Accepted);
    assert_eq!(launches_within(&feed, QUIET), Vec::new());
    runtime.post(Command::Host(HostCommand::Show {
        environment: named("box"),
    }));
    host_connected(&feed)?;
    let heard = launched(&feed)?;
    assert_eq!(heard.terminal, terminal("box", 4));
    assert_eq!(launches_within(&feed, QUIET), Vec::new());
    Ok(())
}

#[test]
fn missing_sessions_are_spawned_and_agents_found_when_the_host_connects() -> TestResult {
    let runtime = start(
        vec![environment("box", Some(CONNECTS), &[5, 6])?],
        Settings::default(),
    )?;
    let feed = follow(&runtime)?;
    let open_missing = TerminalCommand::OpenMissing {
        environment: named("box"),
    };
    assert_eq!(terminal_command(&runtime, open_missing)?, Reply::Accepted);
    assert_eq!(launches_within(&feed, QUIET), Vec::new());
    runtime.post(Command::Host(HostCommand::Show {
        environment: named("box"),
    }));
    host_connected(&feed)?;
    let mut launched_terminals = Vec::new();
    let mut agents = None;
    while launched_terminals.len() < 2 || agents.is_none() {
        match until(&feed, None, |kind| {
            matches!(
                kind,
                EventKind::TerminalLaunched(_) | EventKind::HostAgentsFound { .. }
            )
        })? {
            EventKind::TerminalLaunched(launched) => {
                launched_terminals.push(launched.terminal.terminal.as_u64());
            }
            EventKind::HostAgentsFound { agents: found, .. } => agents = Some(found),
            _ => {}
        }
    }
    launched_terminals.sort_unstable();
    assert_eq!(launched_terminals, [5, 6]);
    assert!(agents.is_some_and(|agents| agents.contains(&AgentPreset::Shell)));
    assert!(runtime.take_launched_view(&terminal("box", 5)).is_some());
    let snapshot = block_on(runtime.snapshot(krad_protocol::Scope::Runtime))?;
    assert!(snapshot.hosts.hosts[0].agents.contains(&AgentPreset::Shell));
    Ok(())
}

#[test]
fn opening_a_terminal_in_an_unknown_environment_is_refused() -> TestResult {
    let runtime = start(Vec::new(), Settings::default())?;
    let open = TerminalCommand::Open {
        terminal: terminal("nowhere", 1),
        intent: LaunchIntent::Preset {
            preset: AgentPreset::Shell,
        },
        cwd: None,
    };
    assert!(matches!(
        terminal_command(&runtime, open)?,
        Reply::Refused {
            code: krad_protocol::ErrorCode::NotFound,
            ..
        }
    ));
    Ok(())
}
