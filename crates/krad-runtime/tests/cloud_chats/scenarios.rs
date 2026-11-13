use krad_agents::{QueuedEvent, Request};
use krad_chat::{new_session_id, LiveSession};
use krad_domain::{
    AgentPreset, AskAnswer, AttentionKind, ChatMessage, CloudProcess, InstallId, JournalOffset,
    NodeEdit, NodeField, Prompt, Speaker, Timestamp,
};
use krad_host::runner::queue_events;
use krad_protocol::{
    AgentRef, ChatAsk, ChatCommand, ChatMirror, CloudAskReplayed, Command, EventKind, NodeCommand,
    Reply, RunnerCommand,
};
use krad_runtime::{ChatBrief, ChatRole, ChatStart, RuntimeHandle};
use std::error::Error;
use std::path::Path;
use std::time::{Duration, Instant};

use super::cloud::{
    cloned_with_remote, create_cloud_session, environment, environment_data, install_id, send,
    start, temp_folder, until, use_fake_runner, Folder, Started, TestResult, RUNNER,
};
use super::mirror::ChatFeed;
use super::repo::{committed_repo, Repo};
use super::runner::{install_host_cli, runner_dir, start_daemon, Daemon};

const PROMPT: &str = "Why does it race?";
const RECORDED_ANSWER: &str = "Two tabs rotate at once.";
const ASKING_PROMPT: &str = "List it";
const QUIET_PROMPT: &str = "Carry on";
const LISTED_ANSWER: &str = "Listed the folder.";
const ASKED_TOOL: &str = "Bash";
const SAVED_WAIT: Duration = Duration::from_secs(20);
const SAVED_POLL: Duration = Duration::from_millis(50);
const NOT_OPENED: &str = "the carried chat never opened";
const NO_PROCESS: &str = "the cloud chat kept no process";
const NO_ASK: &str = "the chat never asked";
const NOT_SAVED: &str = "the cloud chat's offset was never kept";

struct CloudChat {
    started: Started,
    chats: ChatFeed,
    id: AgentRef,
    install: InstallId,
    _daemon: Daemon,
    runner_home: Folder,
    _local: Folder,
    _origin: Repo,
    _config: Folder,
}

fn open_cloud_chat(name: &str) -> Result<CloudChat, Box<dyn Error>> {
    let config = temp_folder(&format!("{name}c"))?;
    let runner_home = temp_folder(&format!("{name}h"))?;
    use_fake_runner(&config.0, &runner_home.0);
    let origin = committed_repo(&format!("{name}-origin"))?;
    let local = cloned_with_remote(&origin, &format!("{name}l"))?;
    install_host_cli(&runner_home.0)?;
    let daemon = start_daemon(&runner_home.0)?;
    let install = install_id()?;
    let started = start(&local.0.join("work"), install.clone())?;
    let mut chats = ChatFeed::follow(&started.runtime);
    let (_, opened) = create_cloud_session(&started)?;
    let EventKind::ChatOpened { chat: id, .. } =
        chats.until(|event| matches!(event, EventKind::ChatOpened { .. }))?
    else {
        return Err(NOT_OPENED.into());
    };
    let data = environment_data(&started.runtime)?;
    assert_eq!(data.session_of(id.terminal), Some(opened.session));
    Ok(CloudChat {
        started,
        chats,
        id,
        install,
        runner_home,
        _daemon: daemon,
        _local: local,
        _origin: origin,
        _config: config,
    })
}

fn sent(id: &AgentRef, text: &str) -> Command {
    Command::Chat(ChatCommand::Send {
        chat: id.clone(),
        prompt: Prompt::from(text),
    })
}

fn answers(mirror: &ChatMirror) -> usize {
    mirror
        .messages
        .iter()
        .filter(|message| message.speaker == Speaker::Assistant && message.text == RECORDED_ANSWER)
        .count()
}

fn settled_with(mirror: &ChatMirror, count: usize) -> bool {
    !mirror.state.working && answers(mirror) == count
}

fn reopened(messages: Vec<ChatMessage>) -> ChatStart {
    ChatStart {
        harness: AgentPreset::ClaudeCode,
        brief: ChatBrief {
            role: ChatRole::Agent,
            ..ChatBrief::default()
        },
        prepared: None,
        session: LiveSession::Start(new_session_id()),
        messages,
        queued: Vec::new(),
        notices: Vec::new(),
    }
}

fn saved_process(runtime: &RuntimeHandle, id: &AgentRef) -> Result<CloudProcess, Box<dyn Error>> {
    environment_data(runtime)?
        .nodes
        .into_iter()
        .find(|node| node.id == id.terminal)
        .and_then(|node| node.cloud)
        .ok_or_else(|| NO_PROCESS.into())
}

fn saved_past(
    runtime: &RuntimeHandle,
    id: &AgentRef,
    offset: JournalOffset,
) -> Result<CloudProcess, Box<dyn Error>> {
    let deadline = Instant::now() + SAVED_WAIT;
    while Instant::now() < deadline {
        let saved = saved_process(runtime, id)?;
        if saved.offset > offset {
            return Ok(saved);
        }
        std::thread::sleep(SAVED_POLL);
    }
    Err(NOT_SAVED.into())
}

fn close(cloud: &mut CloudChat) -> Result<(), Box<dyn Error>> {
    let id = cloud.id.clone();
    cloud.started.runtime.close_chat(id.clone());
    cloud
        .chats
        .until(|event| matches!(event, EventKind::ChatClosed { chat } if *chat == id))?;
    Ok(())
}

fn reopen(cloud: &mut CloudChat, messages: Vec<ChatMessage>) -> Result<(), Box<dyn Error>> {
    let id = cloud.id.clone();
    cloud
        .started
        .runtime
        .open_chat(id.clone(), reopened(messages));
    cloud
        .chats
        .until(|event| matches!(event, EventKind::ChatOpened { chat, .. } if *chat == id))?;
    Ok(())
}

fn keep_offset(
    runtime: &RuntimeHandle,
    id: &AgentRef,
    process: CloudProcess,
) -> Result<Reply, Box<dyn Error>> {
    let edit = NodeEdit::Set {
        terminal: id.terminal,
        field: NodeField::Cloud(Some(process)),
    };
    send(
        runtime,
        Command::Node(NodeCommand {
            environment: environment(),
            edit,
        }),
    )
}

fn asked(chats: &mut ChatFeed, id: &AgentRef) -> Result<ChatAsk, Box<dyn Error>> {
    match chats.until(|event| matches!(event, EventKind::ChatAsk(ask) if ask.chat == *id))? {
        EventKind::ChatAsk(ask) => Ok(ask),
        _ => Err(NO_ASK.into()),
    }
}

fn listed(mirror: &ChatMirror) -> bool {
    mirror
        .messages
        .iter()
        .any(|message| message.speaker == Speaker::Assistant && message.text == LISTED_ANSWER)
}

pub fn a_cloud_chat_answers_on_the_runner_reattaches_from_its_offset_and_replays_its_journal(
) -> TestResult {
    let mut cloud = open_cloud_chat("cc")?;
    let id = cloud.id.clone();
    let runtime = cloud.started.runtime.clone();

    assert_eq!(send(&runtime, sent(&id, PROMPT))?, Reply::Accepted);
    cloud
        .chats
        .until_chat(&id, |mirror| settled_with(mirror, 1))?;
    let first = cloud.chats.mirror(&id)?.messages;
    close(&mut cloud)?;
    let kept = saved_past(&runtime, &id, JournalOffset::default())?;

    reopen(&mut cloud, first)?;
    assert_eq!(send(&runtime, sent(&id, PROMPT))?, Reply::Accepted);
    cloud
        .chats
        .until_chat(&id, |mirror| settled_with(mirror, 2))?;
    let second = cloud.chats.mirror(&id)?.messages;
    close(&mut cloud)?;
    let before_ask = saved_past(&runtime, &id, kept.offset)?;
    assert_eq!(before_ask.id, kept.id);

    reopen(&mut cloud, second.clone())?;
    assert_eq!(send(&runtime, sent(&id, ASKING_PROMPT))?, Reply::Accepted);
    asked(&mut cloud.chats, &id)?;
    close(&mut cloud)?;
    saved_past(&runtime, &id, before_ask.offset)?;

    assert_eq!(keep_offset(&runtime, &id, before_ask)?, Reply::Accepted);
    reopen(&mut cloud, second)?;
    assert_eq!(send(&runtime, sent(&id, QUIET_PROMPT))?, Reply::Accepted);
    let replayed = asked(&mut cloud.chats, &id)?;
    let allow = Command::Chat(ChatCommand::AnswerAsk {
        chat: id.clone(),
        ask: replayed.ask,
        answer: AskAnswer::Allow,
    });
    assert_eq!(send(&runtime, allow)?, Reply::Accepted);
    cloud.chats.until_chat(&id, listed)?;
    assert_eq!(cloud.chats.mirror(&id)?.state.problem, None);
    Ok(())
}

fn queue_ask(
    runner_home: &Path,
    install: &InstallId,
    process: &CloudProcess,
) -> Result<(), Box<dyn Error>> {
    let event = QueuedEvent {
        process: process.id.clone(),
        at: Timestamp::now(),
        request: Request::Ask {
            tool: ASKED_TOOL.to_owned(),
            input: serde_json::json!({ "command": "ls" }),
            suggestions: Vec::new(),
            call: None,
        },
        webhooked: std::collections::BTreeSet::new(),
    };
    let line = serde_json::to_string(&event)?;
    queue_events(&runner_dir(runner_home), &[(install.clone(), line)]);
    Ok(())
}

pub fn a_returning_runner_replays_what_its_chat_asked_while_the_app_was_away() -> TestResult {
    let mut cloud = open_cloud_chat("cr")?;
    let id = cloud.id.clone();
    let runtime = cloud.started.runtime.clone();
    assert_eq!(send(&runtime, sent(&id, PROMPT))?, Reply::Accepted);
    cloud
        .chats
        .until_chat(&id, |mirror| settled_with(mirror, 1))?;
    let process = saved_process(&runtime, &id)?;

    queue_ask(&cloud.runner_home.0, &cloud.install, &process)?;
    let reconnect = Command::Runner(RunnerCommand::Reconnect { runner: RUNNER });
    assert_eq!(send(&runtime, reconnect)?, Reply::Accepted);
    let replayed = until(&cloud.started.feed, |kind| match kind {
        EventKind::CloudAskReplayed(replayed) => Some(replayed),
        _ => None,
    })?;
    assert_eq!(
        replayed,
        CloudAskReplayed {
            agent: id.clone(),
            kind: AttentionKind::Permission,
            delivered: std::collections::BTreeSet::new(),
        }
    );
    assert_eq!(send(&runtime, sent(&id, PROMPT))?, Reply::Accepted);
    cloud
        .chats
        .until_chat(&id, |mirror| settled_with(mirror, 2))?;
    close(&mut cloud)?;
    Ok(())
}
