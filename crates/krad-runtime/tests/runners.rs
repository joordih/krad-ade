#![cfg(unix)]

use futures::executor::block_on;
use futures::StreamExt as _;
use krad_domain::{
    CloudRunner, CloudRunnerId, CloudSettings, OfflineWebhooks, RunnerState, Settings,
};
use krad_protocol::{
    Command, ErrorCode, EventKind, Feed, Reply, RunnerCommand, RunnerRemoved, RunnerShown, Scope,
};
use krad_runtime::{Runtime, RuntimeHandle, RuntimeSetup, SettingsSetup};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

type TestResult = Result<(), Box<dyn Error>>;

const WAIT: Duration = Duration::from_secs(20);
const POLL: Duration = Duration::from_millis(50);
const TIMED_OUT: &str = "the awaited event never came";
const FAKE_SSH_DIR: &str = "tests/fixtures/fake-ssh";
const PATH_VAR: &str = "PATH";
const CONFIG_HOME_VAR: &str = "XDG_CONFIG_HOME";
const REFUSES: &str = "refuses.invalid";
const ASKS: &str = "asks.invalid";
const CONNECTS: &str = "connects.invalid";
const UNKNOWN: CloudRunnerId = CloudRunnerId(99);
const TEST_IDS_FROM: u64 = 7_000_000_000_000;

struct Folder(PathBuf);

impl Drop for Folder {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.0));
    }
}

fn use_fake_ssh_and_config(config: &Path) {
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
    std::env::set_var(CONFIG_HOME_VAR, config);
}

fn runners_on(hosts: &[&str]) -> Result<(CloudSettings, Vec<CloudRunnerId>), Box<dyn Error>> {
    let mut cloud = CloudSettings::default();
    let first = TEST_IDS_FROM + u64::from(std::process::id());
    for (at, host) in (first..).zip(hosts) {
        cloud.runners.push(CloudRunner {
            id: CloudRunnerId(at),
            name: host.parse()?,
            destination: host.parse()?,
            offline_webhooks: OfflineWebhooks::default(),
        });
    }
    let ids = cloud.runners.iter().map(|runner| runner.id).collect();
    Ok((cloud, ids))
}

fn start(cloud: CloudSettings) -> Result<(RuntimeHandle, Receiver<Feed>), Box<dyn Error>> {
    let settings = Settings {
        cloud,
        ..Settings::default()
    };
    let runtime = Runtime::start(RuntimeSetup {
        settings: SettingsSetup {
            settings,
            file: None,
        },
        ..RuntimeSetup::default()
    })?;
    let mut feed = runtime.subscribe();
    let (heard, received) = channel();
    std::thread::spawn(move || {
        while let Some(next) = block_on(feed.next()) {
            if heard.send(next).is_err() {
                break;
            }
        }
    });
    Ok((runtime, received))
}

fn states_of(
    feed: &Receiver<Feed>,
    runner: CloudRunnerId,
) -> Result<Vec<RunnerState>, Box<dyn Error>> {
    let deadline = Instant::now() + WAIT;
    let mut states = Vec::new();
    while Instant::now() < deadline {
        match feed.recv_timeout(POLL) {
            Ok(Feed::Event(event)) => {
                let EventKind::RunnerChanged(RunnerShown { runner: of, state }) = event.kind else {
                    continue;
                };
                if of != runner {
                    continue;
                }
                let settled = state.is_ready() || matches!(state, RunnerState::Failed { .. });
                states.push(state);
                if settled {
                    return Ok(states);
                }
            }
            Ok(Feed::Snapshot(_)) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Err("the feed ended".into()),
        }
    }
    Err(TIMED_OUT.into())
}

fn removal_of(
    feed: &Receiver<Feed>,
    runner: CloudRunnerId,
) -> Result<RunnerRemoved, Box<dyn Error>> {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if let Ok(Feed::Event(event)) = feed.recv_timeout(POLL) {
            if let EventKind::RunnerRemoved(removed) = event.kind {
                if removed.runner == runner {
                    return Ok(removed);
                }
            }
        }
    }
    Err(TIMED_OUT.into())
}

fn send(runtime: &RuntimeHandle, command: RunnerCommand) -> Result<Reply, Box<dyn Error>> {
    Ok(block_on(
        runtime.send(runtime.next_request(), Command::Runner(command)),
    )?)
}

fn failure(states: &[RunnerState]) -> Option<&str> {
    match states.last() {
        Some(RunnerState::Failed { reason }) => Some(reason),
        _ => None,
    }
}

#[test]
fn runners_connect_install_fail_with_a_reason_and_are_removed() -> TestResult {
    let config = Folder(std::env::temp_dir().join(format!(
        "krad-runtime-runners-config-{}",
        std::process::id()
    )));
    use_fake_ssh_and_config(&config.0);
    let (cloud, ids) = runners_on(&[REFUSES, ASKS, CONNECTS])?;
    let [refuses, asks, connects] = ids[..] else {
        return Err("three runners were not added".into());
    };
    let (runtime, feed) = start(cloud)?;

    assert!(matches!(
        send(&runtime, RunnerCommand::Ensure { runner: UNKNOWN })?,
        Reply::Refused {
            code: ErrorCode::NotFound,
            ..
        }
    ));

    assert_eq!(
        send(&runtime, RunnerCommand::Ensure { runner: refuses })?,
        Reply::Accepted
    );
    let refusal = states_of(&feed, refuses)?;
    assert_eq!(refusal.first(), Some(&RunnerState::Connecting));
    assert!(
        failure(&refusal).is_some_and(|reason| reason.contains(REFUSES)),
        "{refusal:?}"
    );

    assert_eq!(
        send(&runtime, RunnerCommand::Ensure { runner: asks })?,
        Reply::Accepted
    );
    let asked = states_of(&feed, asks)?;
    assert!(
        failure(&asked).is_some_and(|reason| reason.contains("asks for a password")),
        "{asked:?}"
    );

    assert_eq!(
        send(&runtime, RunnerCommand::Ensure { runner: connects })?,
        Reply::Accepted
    );
    let installed = states_of(&feed, connects)?;
    assert_eq!(
        installed[..2],
        [RunnerState::Connecting, RunnerState::Installing]
    );
    assert!(
        failure(&installed).is_some_and(|reason| reason.contains("no id")),
        "{installed:?}"
    );
    assert!(runtime.runner(connects).is_none());

    let shown = block_on(runtime.snapshot(Scope::Runtime))?.runners;
    assert_eq!(
        shown.iter().map(|shown| shown.runner).collect::<Vec<_>>(),
        [refuses, asks, connects]
    );

    assert_eq!(
        send(&runtime, RunnerCommand::Remove { runner: connects })?,
        Reply::Accepted
    );
    assert_eq!(removal_of(&feed, connects)?.problem, None);
    let shown = block_on(runtime.snapshot(Scope::Runtime))?.runners;
    assert!(shown.iter().all(|shown| shown.runner != connects));
    Ok(())
}
