use futures::executor::block_on;
use krad_client::{Runner, RuntimeClient};
use krad_domain::{AskAnswer, CloudProcess, EnvironmentData};
use krad_protocol::{
    AgentRef, ChatCommand, CloudCommand, CloudNews, CloudReach, Command, ErrorCode, Reply, Scope,
};
use krad_runtime::RuntimeHandle;
use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::cloud::{
    cloned_with_remote, environment, environment_data, install_id, news, send, start, temp_folder,
    use_fake_runner, TestResult, RUNNER,
};
use super::hosted_view::the_runner_shows_its_hosted_sessions_terminals_and_failures;
use super::mirror::ChatFeed;
use super::repo::committed_repo;
use super::runner::{install_host_cli, runner_dir, start_daemon};
use super::scenarios::{asked, listed, sent, settled_with, ASKING_PROMPT, PROMPT};

const KEPT_WAIT: Duration = Duration::from_secs(20);
const KEPT_POLL: Duration = Duration::from_millis(100);
const STEADY_FOR: Duration = Duration::from_millis(2500);
const STATE_FILE: &str = "runtime/state.json";
const NOT_HOSTED: &str = "the cloud session was not handed to the runner's runtime";
const NO_ENVIRONMENT: &str = "the runner's runtime has no environment";
const NO_PROCESS: &str = "the runner never kept the chat's process";
const NOT_KEPT: &str = "the runner never wrote the chat's process to its state";

type Client = Arc<dyn RuntimeClient>;

fn runner_client(runtime: &RuntimeHandle) -> Result<Client, Box<dyn Error>> {
    let line = block_on(runtime.connect_runner(RUNNER))??;
    Ok(Arc::new(Runner::connect(
        line.input,
        line.output,
        line.hello,
    )?))
}

fn on_runner(client: &Client) -> Result<EnvironmentData, Box<dyn Error>> {
    block_on(client.snapshot(Scope::Runtime))?
        .environments
        .into_iter()
        .next()
        .ok_or_else(|| NO_ENVIRONMENT.into())
}

fn process_on(client: &Client, id: &AgentRef) -> Result<Option<CloudProcess>, Box<dyn Error>> {
    Ok(on_runner(client)?
        .nodes
        .into_iter()
        .find(|node| node.id == id.terminal)
        .and_then(|node| node.cloud))
}

fn steady_process(client: &Client, id: &AgentRef) -> Result<CloudProcess, Box<dyn Error>> {
    let deadline = Instant::now() + KEPT_WAIT;
    let mut last: Option<(CloudProcess, Instant)> = None;
    while Instant::now() < deadline {
        let now = process_on(client, id)?.filter(|kept| kept.offset.0 > 0);
        last = match (now, last) {
            (Some(now), Some((seen, since))) if now == seen => {
                if since.elapsed() >= STEADY_FOR {
                    return Ok(now);
                }
                Some((seen, since))
            }
            (Some(now), _) => Some((now, Instant::now())),
            (None, _) => None,
        };
        std::thread::sleep(KEPT_POLL);
    }
    Err(NO_PROCESS.into())
}

fn written(state: &std::path::Path, process: &CloudProcess) -> TestResult {
    let deadline = Instant::now() + KEPT_WAIT;
    while Instant::now() < deadline {
        let text = std::fs::read_to_string(state).unwrap_or_default();
        if text.contains(process.id.as_str()) {
            return Ok(());
        }
        std::thread::sleep(KEPT_POLL);
    }
    Err(NOT_KEPT.into())
}

fn answer(client: &Client, id: &AgentRef, ask: krad_protocol::ChatAskId) -> Reply {
    let command = Command::Chat(ChatCommand::AnswerAsk {
        chat: id.clone(),
        ask,
        answer: AskAnswer::Allow,
    });
    block_on(client.send(command)).unwrap_or_else(|error| Reply::Refused {
        code: ErrorCode::Failed,
        message: error.to_string(),
    })
}

pub fn a_cloud_session_handed_to_the_runner_s_runtime_chats_there_and_survives_its_restart(
) -> TestResult {
    let config = temp_folder("hc")?;
    let runner_home = temp_folder("hh")?;
    use_fake_runner(&config.0, &runner_home.0);
    let origin = committed_repo("hosted-origin")?;
    let local = cloned_with_remote(&origin, "hl")?;
    install_host_cli(&runner_home.0)?;
    let mut daemon = start_daemon(&runner_home.0)?;
    let started = start(&local.0.join("work"), install_id()?)?;

    let create = CloudCommand::Create {
        environment: environment(),
        group: started.group,
        runner: RUNNER,
        reach: CloudReach::Runtime,
    };
    assert_eq!(
        send(&started.runtime, Command::Cloud(create))?,
        Reply::Accepted
    );
    let hosted = news(&started.feed, |news| {
        matches!(news, CloudNews::Hosted { .. })
    })?;
    if !matches!(&hosted, CloudNews::Hosted { runner, slug } if *runner == RUNNER && !slug.is_empty())
    {
        return Err(format!("{NOT_HOSTED}: {hosted:?}").into());
    }
    let desktop = environment_data(&started.runtime)?;
    assert!(desktop
        .sessions
        .iter()
        .all(|session| session.cloud.is_none()));

    let first = runner_client(&started.runtime)?;
    let second = runner_client(&started.runtime)?;
    let mut chats = ChatFeed::follow_client(Arc::clone(&first));
    let mut watching = ChatFeed::follow_client(Arc::clone(&second));
    let id = chats.until_open()?;
    let runner = on_runner(&first)?;
    let session = runner
        .session(runner.session_of(id.terminal).ok_or(NOT_HOSTED)?)
        .ok_or(NOT_HOSTED)?;
    let placement = session.cloud.clone().ok_or(NOT_HOSTED)?;
    assert!(placement.worktree.starts_with(runner_dir(&runner_home.0)));
    assert!(placement.worktree.exists());

    assert_eq!(block_on(first.send(sent(&id, PROMPT)))?, Reply::Accepted);
    chats.until_chat(&id, |mirror| settled_with(mirror, 1))?;
    watching.until_chat(&id, |mirror| settled_with(mirror, 1))?;

    assert_eq!(
        block_on(first.send(sent(&id, ASKING_PROMPT)))?,
        Reply::Accepted
    );
    let ask = asked(&mut chats, &id)?;
    assert_eq!(asked(&mut watching, &id)?.ask, ask.ask);
    let answers = [answer(&second, &id, ask.ask), answer(&first, &id, ask.ask)];
    assert_eq!(answers[0], Reply::Accepted);
    assert!(matches!(
        &answers[1],
        Reply::Refused {
            code: ErrorCode::AlreadyAnswered,
            ..
        }
    ));
    chats.until_chat(&id, listed)?;
    the_runner_shows_its_hosted_sessions_terminals_and_failures(
        &started.runtime,
        &id,
        started.group,
    )?;

    let kept = steady_process(&first, &id)?;
    written(&runner_dir(&runner_home.0).join(STATE_FILE), &kept)?;
    drop((chats, watching, first, second));
    daemon.restart()?;

    let back = runner_client(&started.runtime)?;
    let mut chats = ChatFeed::follow_client(Arc::clone(&back));
    chats.until_chat(&id, |_| true)?;
    assert_eq!(block_on(back.send(sent(&id, PROMPT)))?, Reply::Accepted);
    chats.until_chat(&id, |mirror| settled_with(mirror, 1))?;
    let resumed = process_on(&back, &id)?.ok_or(NO_PROCESS)?;
    assert_eq!(resumed.id, kept.id);
    Ok(())
}
