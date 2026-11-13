#![cfg(unix)]

#[path = "support/chats.rs"]
mod chat_support;
#[path = "support/gateway.rs"]
mod gateway;
#[path = "support/opencode.rs"]
mod opencode;

use chat_support::{
    chat, claude_start, fake_runner, live_config, start_runtime, start_with, Ask as _, ChatFeed,
    TestResult, CLAUDE_HARNESS,
};
use krad_chat::{new_opencode_session_id, new_session_id, ChatBackend, LiveSession, Runner};
use krad_domain::{AgentPreset, AskAnswer, Host, Prompt, Speaker};
use krad_protocol::{
    AgentRef, ChatCommand, ChatMirror, Command, ErrorCode, EventKind, OpencodeShown,
    OpencodeStatus, Reply,
};
use krad_runtime::ChatLaunch;
use std::path::PathBuf;
use std::process::Command as Process;

const ASK_HARNESS: &str = "tests/fixtures/fake-harness/claude-ask";
const PROMPT: &str = "Why does it race?";
const RECORDED_ANSWER: &str = "Two tabs rotate at once.";
const CODEX_HARNESS: &str = "../krad-chat/tests/fixtures/fake-harness/codex";
const CODEX_THREAD: &str = "019a3c5e-7b21-7d40-9c1e-5f2a8b6d4e10";
const NOTICE: &str = "The chosen model is not offered here; the default one answers";
const OPENCODE_PROGRAM: &str = "opencode";
const OPENCODE_PROMPT: &str = "ping";
const NO_RESUME: &str = "the chat never told its session";

fn send(id: &AgentRef, text: &str) -> Command {
    Command::Chat(ChatCommand::Send {
        chat: id.clone(),
        prompt: Prompt::from(text),
    })
}

fn answered(mirror: &ChatMirror, text: &str) -> bool {
    !mirror.state.working
        && mirror
            .messages
            .iter()
            .any(|message| message.speaker == Speaker::Assistant && message.text == text)
}

fn queued(mirror: &ChatMirror) -> Vec<&str> {
    mirror
        .state
        .queued
        .iter()
        .map(|prompt| prompt.text.as_str())
        .collect()
}

#[test]
fn a_working_chat_tells_since_when_and_keeps_its_notices_apart_from_problems() -> TestResult {
    let runtime = start_runtime()?;
    let mut feed = ChatFeed::follow(&runtime)?;
    let id = chat(11);
    let mut start = claude_start(
        fake_runner(CLAUDE_HARNESS),
        LiveSession::Start(new_session_id()),
    );
    start.notices = vec![NOTICE.to_owned()];
    runtime.open_chat(id.clone(), start);
    feed.until(|event| matches!(event, EventKind::ChatOpened { .. }))?;
    let opened = feed.mirror(&id)?.state;
    assert!(opened.notices.is_empty() && opened.working_since.is_none());
    runtime.send_now(send(&id, PROMPT))?;
    feed.until_chat(&id, |mirror| mirror.state.working)?;
    assert!(feed.mirror(&id)?.state.working_since.is_some());
    feed.until_chat(&id, |mirror| answered(mirror, RECORDED_ANSWER))?;
    let settled = feed.mirror(&id)?.state;
    assert_eq!(settled.working_since, None);
    assert_eq!(settled.notices, [NOTICE]);
    assert_eq!(settled.problem, None);
    Ok(())
}

#[test]
fn prompts_sent_while_working_wait_in_the_runtime_queue_and_can_be_taken_back() -> TestResult {
    let runtime = start_runtime()?;
    let mut feed = ChatFeed::follow(&runtime)?;
    let id = chat(12);
    runtime.open_chat(
        id.clone(),
        claude_start(
            fake_runner(ASK_HARNESS),
            LiveSession::Start(new_session_id()),
        ),
    );
    runtime.send_now(send(&id, "List it"))?;
    let EventKind::ChatAsk(ask) = feed.until(|event| matches!(event, EventKind::ChatAsk(_)))?
    else {
        unreachable!("the awaited event is an ask");
    };
    runtime.send_now(send(&id, "second"))?;
    runtime.send_now(send(&id, "third"))?;
    feed.until_chat(&id, |mirror| queued(mirror) == ["second", "third"])?;
    let unqueue = |index| {
        Command::Chat(ChatCommand::Unqueue {
            chat: id.clone(),
            index,
        })
    };
    assert_eq!(runtime.send_now(unqueue(0))?, Reply::Accepted);
    assert!(matches!(
        runtime.send_now(unqueue(7))?,
        Reply::Refused {
            code: ErrorCode::NotFound,
            ..
        }
    ));
    feed.until_chat(&id, |mirror| queued(mirror) == ["third"])?;
    runtime.send_now(Command::Chat(ChatCommand::AnswerAsk {
        chat: id.clone(),
        ask: ask.ask,
        answer: AskAnswer::Allow,
    }))?;
    feed.until_chat(&id, |mirror| {
        mirror.state.queued.is_empty()
            && mirror
                .messages
                .iter()
                .any(|message| message.speaker == Speaker::User && message.text == "third")
    })?;
    let said: Vec<String> = feed
        .mirror(&id)?
        .messages
        .iter()
        .filter(|message| message.speaker == Speaker::User)
        .map(|message| message.text.clone())
        .collect();
    assert_eq!(said, ["List it", "third"]);
    Ok(())
}

fn git_repo() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let repo = std::env::temp_dir().join(format!(
        "krad-runtime-codex-{}-{}",
        std::process::id(),
        new_session_id().as_str()
    ));
    std::fs::create_dir_all(&repo)?;
    std::fs::write(repo.join("notes.txt"), "first\n")?;
    let status = Process::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["init", "--quiet"])
        .status()?;
    if !status.success() {
        return Err("git init failed".into());
    }
    Ok(repo)
}

#[test]
fn opening_a_codex_chat_sends_a_message_emits_the_reply_and_keeps_a_checkpoint() -> TestResult {
    let repo = git_repo()?;
    let runtime = start_runtime()?;
    let mut feed = ChatFeed::follow(&runtime)?;
    let id = chat(13);
    let mut config = live_config(fake_runner(CODEX_HARNESS));
    config.cwd.clone_from(&repo);
    let mut start = start_with(
        AgentPreset::Codex,
        ChatLaunch::from(ChatBackend::Codex(config)),
        LiveSession::Start(new_session_id()),
    );
    if let Some(prepared) = start.prepared.as_mut() {
        prepared.place.root = Some(repo.clone());
    }
    runtime.open_chat(id.clone(), start);
    assert_eq!(runtime.send_now(send(&id, PROMPT))?, Reply::Accepted);
    feed.until_chat(&id, |mirror| answered(mirror, RECORDED_ANSWER))?;
    let mirror = feed.mirror(&id)?;
    assert_eq!(mirror.messages[0].text, PROMPT);
    assert_eq!(mirror.state.harness, AgentPreset::Codex);
    assert_eq!(
        mirror
            .state
            .resume
            .as_ref()
            .map(krad_domain::ResumeId::as_str),
        Some(CODEX_THREAD)
    );
    assert!(mirror.messages[0].rewind.is_some());
    assert!(feed.saw(|event| matches!(event, EventKind::ChatCheckpointed { .. })));
    assert_eq!(ChatMirror::from(runtime.chat_snapshot(&id)?), mirror);
    drop(std::fs::remove_dir_all(repo));
    Ok(())
}

#[test]
fn a_gateway_chat_streams_its_answer_as_appended_text_and_never_tells_its_key() -> TestResult {
    let root = gateway::scratch()?;
    let url = gateway::serve(gateway::ANSWER)?;
    let runtime = start_runtime()?;
    let mut feed = ChatFeed::follow(&runtime)?;
    let id = chat(14);
    runtime.open_chat(
        id.clone(),
        start_with(
            AgentPreset::Gateway,
            ChatLaunch::from(ChatBackend::Gateway(
                gateway::access(&url)?,
                gateway::config(&root),
            )),
            LiveSession::Start(new_session_id()),
        ),
    );
    runtime.send_now(send(&id, "say it"))?;
    feed.until_chat(&id, |mirror| answered(mirror, gateway::ANSWERED))?;
    assert!(feed.saw(|event| matches!(event, EventKind::ChatMessageAppended { .. })));
    let mirror = feed.mirror(&id)?;
    assert_eq!(ChatMirror::from(runtime.chat_snapshot(&id)?), mirror);
    assert!(!feed
        .saw(|event| serde_json::to_string(event).is_ok_and(|json| json.contains(gateway::KEY))));
    drop(std::fs::remove_dir_all(root));
    Ok(())
}

#[test]
fn a_gateway_chat_waits_for_the_runtime_to_reach_the_gateway_before_it_asks_anything() -> TestResult
{
    let root = gateway::scratch()?;
    let runtime = start_runtime()?;
    let mut feed = ChatFeed::follow(&runtime)?;
    let id = chat(15);
    runtime.open_chat(
        id.clone(),
        start_with(
            AgentPreset::Gateway,
            ChatLaunch::Gateway(Box::new(gateway::config(&root))),
            LiveSession::Start(new_session_id()),
        ),
    );
    assert_eq!(runtime.send_now(send(&id, "say it"))?, Reply::Accepted);
    feed.until_chat(&id, |mirror| mirror.state.problem.is_some())?;
    let mirror = feed.mirror(&id)?;
    assert!(!mirror.state.working && !mirror.state.alive);
    assert_eq!(queued(&mirror), ["say it"]);
    drop(std::fs::remove_dir_all(root));
    Ok(())
}

fn opencode_launch() -> ChatLaunch {
    ChatLaunch::Opencode(Box::new(live_config(Runner::Native(PathBuf::from(
        OPENCODE_PROGRAM,
    )))))
}

fn said(mirror: &ChatMirror, speaker: Speaker, text: &str) -> bool {
    mirror
        .messages
        .iter()
        .any(|message| message.speaker == speaker && message.text == text)
}

#[test]
fn an_opencode_chat_starts_its_host_server_answers_and_reads_its_history_back() -> TestResult {
    let state_home = std::env::temp_dir().join(format!(
        "krad-runtime-opencode-{}",
        new_opencode_session_id().as_str()
    ));
    let url = opencode::serve()?;
    opencode::register(&state_home, &url)?;
    let runtime = start_runtime()?;
    let mut feed = ChatFeed::follow(&runtime)?;
    let id = chat(21);
    runtime.open_chat(
        id.clone(),
        start_with(
            AgentPreset::Opencode,
            opencode_launch(),
            LiveSession::Start(new_opencode_session_id()),
        ),
    );
    let sent = runtime.send_now(Command::Chat(ChatCommand::Send {
        chat: id.clone(),
        prompt: Prompt::from(OPENCODE_PROMPT),
    }))?;
    assert_eq!(sent, Reply::Accepted);
    feed.until_chat(&id, |mirror| {
        !mirror.state.working && said(mirror, Speaker::Assistant, opencode::ANSWER)
    })?;
    assert!(feed.saw(|event| matches!(
        event,
        EventKind::OpencodeServer(OpencodeShown {
            host: Host::Local,
            status: OpencodeStatus::Ready,
        })
    )));
    assert!(feed.saw(|event| matches!(
        event,
        EventKind::ModelsListed { listed, .. }
            if listed.harness == AgentPreset::Opencode
                && listed.models.iter().any(|offer| offer.value == opencode::MODEL)
    )));
    assert!(runtime.opencode_server(&Host::Local).is_some());
    let state = feed.mirror(&id)?.state;
    assert!(state
        .commands
        .iter()
        .any(|offered| offered.name == opencode::COMMAND));
    let resume = state.resume.ok_or(NO_RESUME)?;
    assert!(resume.as_str().starts_with("ses_"));

    let again = chat(22);
    runtime.open_chat(
        again.clone(),
        start_with(
            AgentPreset::Opencode,
            opencode_launch(),
            LiveSession::Resume(resume),
        ),
    );
    feed.until_chat(&again, |mirror| {
        mirror.state.origin.is_some()
            && said(mirror, Speaker::User, OPENCODE_PROMPT)
            && said(mirror, Speaker::Assistant, opencode::ANSWER)
    })?;
    assert_eq!(
        feed.mirror(&again)?,
        ChatMirror::from(runtime.chat_snapshot(&again)?)
    );
    std::fs::remove_dir_all(&state_home)?;
    Ok(())
}
