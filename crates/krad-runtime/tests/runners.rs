#![cfg(unix)]

use futures::executor::block_on;
use futures::StreamExt as _;
use krad_domain::{
    ClaudeOauthToken, CloudRunner, CloudRunnerId, CloudSettings, EnvironmentName, OfflineWebhooks,
    PairingToken, ProcessRun, RunnerState, Settings, TerminalId,
};
use krad_protocol::{
    AgentRef, Command, ErrorCode, EventKind, Feed, Reply, RunnerCommand, RunnerRemoved,
    RunnerShown, Scope,
};
use krad_runtime::{Runtime, RuntimeHandle, RuntimeSetup, SettingsSetup};
use krad_store::Secrets;
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
const SIGN_IN_MARK_VAR: &str = "KRAD_FAKE_SIGN_IN_MARK";
const SIGN_IN_MARK: &str = "signed-in";
const REFUSES: &str = "refuses.invalid";
const ASKS: &str = "asks.invalid";
const DECLINES: &str = "declines.invalid";
const SIGNS_IN: &str = "signs-in.invalid";
const CONNECTS: &str = "connects.invalid";
const SIGN_IN_ENVIRONMENT: &str = "main";
const UNKNOWN: CloudRunnerId = CloudRunnerId(99);
const PAIRING: &str = "00112233445566778899aabbccddeeff";
const CLAUDE_TOKEN: &str = "sk-ant-oat01-test";

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
    std::env::set_var(SIGN_IN_MARK_VAR, config.join(SIGN_IN_MARK));
}

fn runners_on(hosts: &[&str]) -> Result<(CloudSettings, Vec<CloudRunnerId>), Box<dyn Error>> {
    let mut cloud = CloudSettings::default();
    for (at, host) in (1..).zip(hosts) {
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

fn start(
    cloud: CloudSettings,
    secrets: Secrets,
) -> Result<(RuntimeHandle, Receiver<Feed>), Box<dyn Error>> {
    let settings = Settings {
        cloud,
        ..Settings::default()
    };
    let runtime = Runtime::start(RuntimeSetup {
        settings: SettingsSetup {
            settings,
            file: None,
        },
        secrets,
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
                let settled = state.is_ready()
                    || matches!(state, RunnerState::Failed { .. } | RunnerState::SigningIn);
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

fn sign_in_wanted(feed: &Receiver<Feed>, runner: CloudRunnerId) -> TestResult {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if let Ok(Feed::Event(event)) = feed.recv_timeout(POLL) {
            if event.kind == (EventKind::RunnerSignInWanted { runner }) {
                return Ok(());
            }
        }
    }
    Err(TIMED_OUT.into())
}

fn sign_in_terminal(id: u64) -> AgentRef {
    AgentRef {
        environment: EnvironmentName::new(SIGN_IN_ENVIRONMENT.to_owned()),
        terminal: TerminalId(id),
    }
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

fn signs_in_over_ssh(
    runtime: &RuntimeHandle,
    feed: &Receiver<Feed>,
    [refuses, asks, declines, signs_in]: [CloudRunnerId; 4],
) -> TestResult {
    assert_eq!(
        send(runtime, RunnerCommand::Ensure { runner: asks })?,
        Reply::Accepted
    );
    let asked = states_of(feed, asks)?;
    assert_eq!(asked, [RunnerState::Connecting, RunnerState::SigningIn]);
    sign_in_wanted(feed, asks)?;
    let stray = runtime.open_runner_ssh_sign_in(sign_in_terminal(1), refuses);
    let view = runtime.open_runner_ssh_sign_in(sign_in_terminal(2), asks);
    let not_kept = states_of(feed, asks)?;
    assert_eq!(not_kept.first(), Some(&RunnerState::Connecting));
    assert!(
        failure(&not_kept).is_some_and(|reason| reason.contains("did not stay open")),
        "{not_kept:?}"
    );
    assert_eq!(view.status().run, ProcessRun::Exited { code: Some(0) });
    assert!(matches!(stray.status().run, ProcessRun::Failed { .. }));

    assert_eq!(
        send(runtime, RunnerCommand::Ensure { runner: declines })?,
        Reply::Accepted
    );
    states_of(feed, declines)?;
    sign_in_wanted(feed, declines)?;
    drop(runtime.open_runner_ssh_sign_in(sign_in_terminal(3), declines));
    let refused_sign_in = states_of(feed, declines)?;
    assert!(
        failure(&refused_sign_in).is_some_and(|reason| reason.contains("code 255")),
        "{refused_sign_in:?}"
    );

    assert_eq!(
        send(runtime, RunnerCommand::Ensure { runner: signs_in })?,
        Reply::Accepted
    );
    assert_eq!(
        states_of(feed, signs_in)?,
        [RunnerState::Connecting, RunnerState::SigningIn]
    );
    sign_in_wanted(feed, signs_in)?;
    drop(runtime.open_runner_ssh_sign_in(sign_in_terminal(4), signs_in));
    let resumed = states_of(feed, signs_in)?;
    assert_eq!(
        resumed[..2],
        [RunnerState::Connecting, RunnerState::Installing]
    );
    assert!(
        failure(&resumed).is_some_and(|reason| reason.contains("no id")),
        "{resumed:?}"
    );
    let terminals = block_on(runtime.snapshot(Scope::Runtime))?.terminals;
    assert!(terminals
        .iter()
        .all(|summary| summary.terminal != sign_in_terminal(4)));
    Ok(())
}

#[test]
fn runners_connect_install_fail_with_a_reason_and_are_removed() -> TestResult {
    let config = Folder(std::env::temp_dir().join(format!(
        "krad-runtime-runners-config-{}",
        std::process::id()
    )));
    std::fs::create_dir_all(&config.0)?;
    use_fake_ssh_and_config(&config.0);
    let (cloud, ids) = runners_on(&[REFUSES, ASKS, CONNECTS, DECLINES, SIGNS_IN])?;
    let [refuses, asks, connects, declines, signs_in] = ids[..] else {
        return Err("five runners were not added".into());
    };
    let secrets = Secrets::in_memory();
    secrets.write_cloud_runner_token(connects, &PAIRING.parse::<PairingToken>()?)?;
    secrets.write_cloud_claude_token(connects, &CLAUDE_TOKEN.parse::<ClaudeOauthToken>()?)?;
    let (runtime, feed) = start(cloud, secrets.clone())?;

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

    signs_in_over_ssh(&runtime, &feed, [refuses, asks, declines, signs_in])?;

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

    let shown = block_on(runtime.snapshot(Scope::Runtime))?.runners.shown;
    assert_eq!(
        shown.iter().map(|shown| shown.runner).collect::<Vec<_>>(),
        [refuses, asks, connects, declines, signs_in]
    );

    assert_eq!(
        send(&runtime, RunnerCommand::Remove { runner: connects })?,
        Reply::Accepted
    );
    assert_eq!(removal_of(&feed, connects)?.problem, None);
    assert!(secrets.find_cloud_runner_token(connects)?.is_none());
    assert!(secrets.find_cloud_claude_token(connects)?.is_none());
    let shown = block_on(runtime.snapshot(Scope::Runtime))?.runners.shown;
    assert!(shown.iter().all(|shown| shown.runner != connects));
    Ok(())
}
