use futures::executor::block_on;
use futures::StreamExt as _;
use krad_domain::{
    CloudRunner, CloudRunnerId, CloudSettings, OfflineWebhooks, PromptAsker, PromptKind,
    RunnerState, Settings,
};
use krad_protocol::{
    Command, EventKind, Feed, HostCommand, HostPrompt, PromptAnswer, Reply, RunnerCommand,
    RunnerShown,
};
use krad_runtime::{HostTools, Runtime, RuntimeHandle, RuntimeSetup, SettingsSetup};
use krad_store::Secrets;
use std::error::Error;
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use super::cloud::{send, temp_folder, TestResult};
use super::runner::askpass_cli;

const WAIT: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(20);
const QUIET: Duration = Duration::from_secs(2);
const TIMED_OUT: &str = "the awaited event never came";
const FEED_ENDED: &str = "the feed ended";
const FAKE_SSH_DIR: &str = "tests/fixtures/fake-ssh";
const PATH_VAR: &str = "PATH";
const CONFIG_HOME_VAR: &str = "XDG_CONFIG_HOME";
const SIGN_IN_MARK_VAR: &str = "KRAD_FAKE_SIGN_IN_MARK";
const PASSWORD_VAR: &str = "KRAD_FAKE_SSH_PASSWORD";
const SIGN_IN_MARK: &str = "signed-in";
const CLI_DIR: &str = "cli";
const PASSWORD: &str = "correct horse";
const WRONG: &str = "battery staple";
const DESTINATION: &str = "askpass.invalid";
const RUNNER: CloudRunnerId = CloudRunnerId(7);
const CANCELLED: &str = "Sign-in cancelled";
const GAVE_UP: &str = "Permission denied (publickey,password)";
const NO_INSTALL_ID: &str = "no id";

enum Heard {
    Prompted(HostPrompt),
    Moved(RunnerState),
}

fn use_fake_ssh(config: &Path) {
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
    std::env::set_var(PASSWORD_VAR, PASSWORD);
}

fn start(config: &Path) -> Result<(RuntimeHandle, Receiver<Feed>), Box<dyn Error>> {
    let cli_dir = config.join(CLI_DIR);
    std::fs::create_dir_all(&cli_dir)?;
    let mut cloud = CloudSettings::default();
    cloud.runners.push(CloudRunner {
        id: RUNNER,
        name: DESTINATION.parse()?,
        destination: DESTINATION.parse()?,
        offline_webhooks: OfflineWebhooks::default(),
    });
    let runtime = Runtime::start(RuntimeSetup {
        settings: SettingsSetup {
            settings: Settings {
                cloud,
                ..Settings::default()
            },
            file: None,
        },
        secrets: Secrets::in_memory(),
        host_tools: HostTools {
            config_dir: Some(config.to_path_buf()),
            local_cli: Some(askpass_cli(&cli_dir)?),
            ..HostTools::default()
        },
        ..RuntimeSetup::default()
    })?;
    let mut events = runtime.subscribe();
    let (heard, feed) = channel();
    std::thread::spawn(move || {
        while let Some(next) = block_on(events.next()) {
            if heard.send(next).is_err() {
                break;
            }
        }
    });
    Ok((runtime, feed))
}

fn next_within(feed: &Receiver<Feed>, wait: Duration) -> Result<Heard, Box<dyn Error>> {
    let deadline = Instant::now() + wait;
    while Instant::now() < deadline {
        match feed.recv_timeout(POLL) {
            Ok(Feed::Event(event)) => match event.kind {
                EventKind::HostPrompted(prompt) => return Ok(Heard::Prompted(prompt)),
                EventKind::RunnerChanged(RunnerShown { runner, state }) if runner == RUNNER => {
                    return Ok(Heard::Moved(state));
                }
                _ => {}
            },
            Ok(Feed::Snapshot(_)) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Err(FEED_ENDED.into()),
        }
    }
    Err(TIMED_OUT.into())
}

fn next(feed: &Receiver<Feed>) -> Result<Heard, Box<dyn Error>> {
    next_within(feed, WAIT)
}

fn answer(runtime: &RuntimeHandle, prompt: &HostPrompt, text: Option<&str>) -> TestResult {
    let reply = send(
        runtime,
        Command::Host(HostCommand::AnswerPrompt {
            prompt: prompt.prompt,
            answer: text.map(|text| PromptAnswer::new(text.to_owned())),
        }),
    )?;
    assert_eq!(reply, Reply::Accepted);
    Ok(())
}

fn reach(runtime: &RuntimeHandle, command: RunnerCommand) -> TestResult {
    assert_eq!(send(runtime, Command::Runner(command))?, Reply::Accepted);
    Ok(())
}

fn signing_in(feed: &Receiver<Feed>) -> TestResult {
    let mut moved = Vec::new();
    while moved.last() != Some(&RunnerState::SigningIn) {
        match next(feed)? {
            Heard::Moved(state) => moved.push(state),
            Heard::Prompted(prompt) => return Err(format!("asked early: {}", prompt.text).into()),
        }
    }
    assert_eq!(moved, [RunnerState::Connecting, RunnerState::SigningIn]);
    Ok(())
}

fn asked(feed: &Receiver<Feed>) -> Result<HostPrompt, Box<dyn Error>> {
    match next(feed)? {
        Heard::Prompted(prompt) => {
            assert_eq!(prompt.asker, PromptAsker::Runner { runner: RUNNER });
            assert_eq!(prompt.kind, PromptKind::Secret);
            assert!(prompt.text.contains(DESTINATION), "{}", prompt.text);
            Ok(prompt)
        }
        Heard::Moved(state) => Err(format!("moved before asking: {state:?}").into()),
    }
}

fn failed_with(state: &RunnerState, wanted: &str) -> bool {
    matches!(state, RunnerState::Failed { reason } if reason.contains(wanted))
}

fn cancelling_fails_the_runner_and_nothing_asks_again(
    runtime: &RuntimeHandle,
    feed: &Receiver<Feed>,
) -> TestResult {
    reach(runtime, RunnerCommand::Ensure { runner: RUNNER })?;
    signing_in(feed)?;
    let prompt = asked(feed)?;
    answer(runtime, &prompt, None)?;
    match next(feed)? {
        Heard::Moved(state) if failed_with(&state, CANCELLED) => {}
        Heard::Moved(state) => return Err(format!("not cancelled: {state:?}").into()),
        Heard::Prompted(prompt) => return Err(format!("asked again: {}", prompt.text).into()),
    }
    match next_within(feed, QUIET) {
        Err(error) if error.to_string() == TIMED_OUT => Ok(()),
        Err(error) => Err(error),
        Ok(Heard::Moved(state)) => Err(format!("moved after cancelling: {state:?}").into()),
        Ok(Heard::Prompted(prompt)) => Err(format!("asked again: {}", prompt.text).into()),
    }
}

fn wrong_answers_ask_again_until_ssh_gives_up(
    runtime: &RuntimeHandle,
    feed: &Receiver<Feed>,
) -> TestResult {
    reach(runtime, RunnerCommand::Reconnect { runner: RUNNER })?;
    signing_in(feed)?;
    let mut asked_times = 0;
    loop {
        match next(feed)? {
            Heard::Prompted(prompt) => {
                asked_times += 1;
                answer(runtime, &prompt, Some(WRONG))?;
            }
            Heard::Moved(state) if failed_with(&state, GAVE_UP) => break,
            Heard::Moved(state) => return Err(format!("did not give up: {state:?}").into()),
        }
    }
    assert_eq!(asked_times, 3);
    Ok(())
}

fn the_right_answer_signs_in_and_installs(
    runtime: &RuntimeHandle,
    feed: &Receiver<Feed>,
) -> TestResult {
    reach(runtime, RunnerCommand::Reconnect { runner: RUNNER })?;
    signing_in(feed)?;
    let first = asked(feed)?;
    answer(runtime, &first, Some(WRONG))?;
    let second = asked(feed)?;
    assert_ne!(second.prompt, first.prompt);
    answer(runtime, &second, Some(PASSWORD))?;
    let mut moved = Vec::new();
    while !moved
        .last()
        .is_some_and(|state| matches!(state, RunnerState::Failed { .. }))
    {
        match next(feed)? {
            Heard::Moved(state) => moved.push(state),
            Heard::Prompted(prompt) => return Err(format!("asked again: {}", prompt.text).into()),
        }
    }
    assert_eq!(
        moved[..2],
        [RunnerState::Connecting, RunnerState::Installing]
    );
    assert!(
        moved
            .last()
            .is_some_and(|state| failed_with(state, NO_INSTALL_ID)),
        "{moved:?}"
    );
    Ok(())
}

pub fn a_cloud_runner_asks_for_its_password_in_the_app_and_signs_in_with_the_answer() -> TestResult
{
    let config = temp_folder("runner-sign-in")?;
    use_fake_ssh(&config.0);
    let (runtime, feed) = start(&config.0)?;
    cancelling_fails_the_runner_and_nothing_asks_again(&runtime, &feed)?;
    wrong_answers_ask_again_until_ssh_gives_up(&runtime, &feed)?;
    the_right_answer_signs_in_and_installs(&runtime, &feed)?;
    Ok(())
}
