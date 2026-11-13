#![cfg(unix)]

#[path = "chats/briefed.rs"]
mod briefed;
#[path = "support/chats.rs"]
mod chat_support;

use chat_support::{
    chat, claude_start, fake_runner, start_runtime, start_with, Ask as _, ChatFeed, TestResult,
    CLAUDE_HARNESS,
};
use krad_chat::acp::OpenBuffers;
use krad_chat::{new_session_id, AcpConfig, AcpWanted, BusVars, LiveSession};
use krad_domain::{
    AcpAgent, AcpAgentStart, AcpModeId, AgentPreset, AnswerPart, AskAnswer, AskKind, AskVerdict,
    AttentionKind, ChatEntry, ChatEvent, Prompt, ResumeId, Speaker,
};
use krad_protocol::{
    AgentRef, ChatAskId, ChatCommand, ChatMirror, Command, ErrorCode, EventKind, Reply,
};
use krad_runtime::ChatLaunch;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const ASK_HARNESS: &str = "tests/fixtures/fake-harness/claude-ask";
const PROMPT: &str = "Why does it race?";
const RECORDED_ANSWER: &str = "Two tabs rotate at once.";
const RECORDED_SESSION: &str = "0b7c3a52-9f1e-4d2a-8c6b-1f2e3d4c5b6a";
const ASKED_ANSWER: &str = "Listed the folder.";
const HOME_VAR: &str = "HOME";
const RECORDED_TURN: &str = "../krad-chat/tests/fixtures/fake-harness/claude-turn.jsonl";
const PROJECTS: &str = ".claude/projects/fake-project";
const CONVERSATION_EXTENSION: &str = "jsonl";
const ACP_AGENT: &str = "tests/fixtures/fake-harness/acp-agent";
const ACP_ANSWER: &str = "pong in code mode";
const CONFIG_HOME_VAR: &str = "XDG_CONFIG_HOME";
const NO_AGENT_ID: &str = "the fake agent id is valid";
const NO_RESUME: &str = "the ACP chat never told its session";
const NO_TRANSCRIPT: &str = "the runtime keeps no ACP transcript without a config folder";

struct NoBuffers;

impl OpenBuffers for NoBuffers {
    fn unsaved_text(&self, _: &Path) -> Option<String> {
        None
    }

    fn written(&self, _: &Path, _: &str) {}
}

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

#[test]
fn opening_a_claude_chat_sends_a_message_and_emits_the_reply() -> TestResult {
    let runtime = start_runtime()?;
    let mut feed = ChatFeed::follow(&runtime)?;
    let id = chat(1);
    runtime.open_chat(
        id.clone(),
        claude_start(
            fake_runner(CLAUDE_HARNESS),
            LiveSession::Start(new_session_id()),
        ),
    );
    feed.until(|event| matches!(event, EventKind::ChatOpened { .. }))?;
    assert_eq!(runtime.send_now(send(&id, PROMPT))?, Reply::Accepted);
    feed.until_chat(&id, |mirror| answered(mirror, RECORDED_ANSWER))?;
    let mirror = feed.mirror(&id)?;
    assert_eq!(mirror.messages.len(), 2);
    assert_eq!(mirror.messages[0].text, PROMPT);
    assert!(!mirror.messages[1].running);
    assert_eq!(
        mirror.state.resume.as_ref().map(ResumeId::as_str),
        Some(RECORDED_SESSION)
    );
    assert!(feed.saw(|event| matches!(event, EventKind::ChatMessageChanged { index: 1, .. })));
    let summary = runtime.chat_snapshot(&id)?;
    assert_eq!(ChatMirror::from(summary), mirror);
    Ok(())
}

#[test]
fn an_ask_waits_for_its_answer_and_the_first_answer_wins() -> TestResult {
    let runtime = start_runtime()?;
    let mut feed = ChatFeed::follow(&runtime)?;
    let id = chat(2);
    runtime.open_chat(
        id.clone(),
        claude_start(
            fake_runner(ASK_HARNESS),
            LiveSession::Start(new_session_id()),
        ),
    );
    runtime.send_now(send(&id, "List it"))?;
    let asked = feed.until(|event| matches!(event, EventKind::ChatAsk(_)))?;
    let EventKind::ChatAsk(ask) = asked else {
        unreachable!("the awaited event is an ask");
    };
    assert!(matches!(&ask.kind, AskKind::Permission { tool, .. } if tool == "Bash"));
    let raised = feed.until(|event| matches!(event, EventKind::AttentionRaised(_)))?;
    assert!(matches!(
        raised,
        EventKind::AttentionRaised(notice) if notice.kind == AttentionKind::Permission
    ));
    assert!(feed.mirror(&id)?.state.working);
    let answer = |ask| {
        Command::Chat(ChatCommand::AnswerAsk {
            chat: id.clone(),
            ask,
            answer: AskAnswer::Allow,
        })
    };
    assert_eq!(runtime.send_now(answer(ask.ask))?, Reply::Accepted);
    let again = runtime.send_now(answer(ask.ask))?;
    assert!(matches!(
        again,
        Reply::Refused {
            code: ErrorCode::AlreadyAnswered,
            ..
        }
    ));
    let unknown = runtime.send_now(answer(ChatAskId(ask.ask.0 + 1_000)))?;
    assert!(matches!(
        unknown,
        Reply::Refused {
            code: ErrorCode::NotFound,
            ..
        }
    ));
    feed.until_chat(&id, |mirror| answered(mirror, ASKED_ANSWER))?;
    let mirror = feed.mirror(&id)?;
    assert_eq!(mirror.asks.len(), 0);
    let verdicts: Vec<AskVerdict> = mirror.messages[1]
        .parts
        .iter()
        .filter_map(|part| match part {
            AnswerPart::Ask(outcome) => Some(outcome.verdict),
            _ => None,
        })
        .collect();
    assert_eq!(verdicts, [AskVerdict::Allowed]);
    Ok(())
}

#[test]
fn commands_for_an_unknown_chat_are_refused() -> TestResult {
    let runtime = start_runtime()?;
    let reply = runtime.send_now(send(&chat(9), PROMPT))?;
    assert!(matches!(
        reply,
        Reply::Refused {
            code: ErrorCode::NotFound,
            ..
        }
    ));
    let reply = runtime.send_now(Command::Chat(ChatCommand::Unknown))?;
    assert!(matches!(
        reply,
        Reply::Refused {
            code: ErrorCode::Unsupported,
            ..
        }
    ));
    Ok(())
}

#[test]
fn a_closed_chat_leaves_the_snapshot() -> TestResult {
    let runtime = start_runtime()?;
    let mut feed = ChatFeed::follow(&runtime)?;
    let id = chat(3);
    runtime.open_chat(
        id.clone(),
        claude_start(
            fake_runner(CLAUDE_HARNESS),
            LiveSession::Start(new_session_id()),
        ),
    );
    feed.until(|event| matches!(event, EventKind::ChatOpened { .. }))?;
    let closing = Command::Chat(ChatCommand::Close { chat: id.clone() });
    assert_eq!(runtime.send_now(closing)?, Reply::Accepted);
    feed.until(|event| matches!(event, EventKind::ChatClosed { .. }))?;
    assert!(runtime.chat_snapshot(&id).is_err());
    Ok(())
}

struct Home(PathBuf);

impl Drop for Home {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.0));
    }
}

fn said(mirror: &ChatMirror) -> Vec<(Speaker, String)> {
    mirror
        .messages
        .iter()
        .map(|message| (message.speaker, message.text.clone()))
        .collect()
}

#[test]
fn a_chat_resumed_after_a_restart_shows_its_history_and_answers_again() -> TestResult {
    let home = Home(std::env::temp_dir().join(format!("krad-chat-resume-{}", std::process::id())));
    std::fs::create_dir_all(home.0.join(PROJECTS))?;
    std::env::set_var(HOME_VAR, &home.0);
    let id = chat(5);
    let resume: ResumeId = {
        let runtime = start_runtime()?;
        let mut feed = ChatFeed::follow(&runtime)?;
        runtime.open_chat(
            id.clone(),
            claude_start(
                fake_runner(CLAUDE_HARNESS),
                LiveSession::Start(new_session_id()),
            ),
        );
        runtime.send_now(Command::Chat(ChatCommand::Send {
            chat: id.clone(),
            prompt: Prompt::from(PROMPT),
        }))?;
        feed.until_chat(&id, |mirror| answered(mirror, RECORDED_ANSWER))?;
        feed.mirror(&id)?
            .state
            .resume
            .ok_or("the chat reported no session")?
    };
    let transcript = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(RECORDED_TURN);
    let conversation = home
        .0
        .join(PROJECTS)
        .join(resume.as_str())
        .with_extension(CONVERSATION_EXTENSION);
    std::fs::copy(transcript, conversation)?;

    let runtime = start_runtime()?;
    let mut feed = ChatFeed::follow(&runtime)?;
    runtime.open_chat(
        id.clone(),
        claude_start(
            fake_runner(CLAUDE_HARNESS),
            LiveSession::Resume(resume.clone()),
        ),
    );
    feed.until_chat(&id, |mirror| mirror.messages.len() == 2)?;
    assert_eq!(
        said(&feed.mirror(&id)?),
        [
            (Speaker::User, PROMPT.to_owned()),
            (Speaker::Assistant, RECORDED_ANSWER.to_owned())
        ]
    );
    let sent = runtime.send_now(Command::Chat(ChatCommand::Send {
        chat: id.clone(),
        prompt: Prompt::from(PROMPT),
    }))?;
    assert_eq!(sent, Reply::Accepted);
    feed.until_chat(&id, |mirror| {
        !mirror.state.working && mirror.messages.len() == 4 && !mirror.messages[3].running
    })?;
    let mirror = feed.mirror(&id)?;
    assert_eq!(mirror.messages[3].text, RECORDED_ANSWER);
    assert_eq!(mirror.state.resume, Some(resume));
    assert_eq!(ChatMirror::from(runtime.chat_snapshot(&id)?), mirror);
    Ok(())
}

fn acp_start() -> Result<krad_runtime::ChatStart, Box<dyn std::error::Error>> {
    let agent = AcpAgent {
        id: "fake".parse().map_err(|_| NO_AGENT_ID)?,
        name: "Fake".to_owned(),
        program: "acp-agent".to_owned(),
        args: Vec::new(),
        env: Vec::new(),
        terminal: None,
    };
    let harness = AgentPreset::Acp(agent.id);
    let config = AcpConfig {
        runner: fake_runner(ACP_AGENT),
        agent: AcpAgentStart {
            agent,
            secrets: std::collections::HashMap::new(),
        },
        cwd: std::env::temp_dir(),
        mcp: Vec::new(),
        bus_vars: BusVars::default(),
        buffers: Arc::new(NoBuffers),
        model: None,
        wanted: AcpWanted::default(),
    };
    Ok(start_with(
        harness,
        ChatLaunch::Acp(Box::new(config)),
        LiveSession::Start(new_session_id()),
    ))
}

#[test]
fn an_acp_chat_sends_the_mode_chosen_early_reads_its_secrets_and_keeps_a_transcript() -> TestResult
{
    let config_home = Home(std::env::temp_dir().join(format!("krad-acp-{}", std::process::id())));
    std::env::set_var(CONFIG_HOME_VAR, &config_home.0);
    let runtime = start_runtime()?;
    let mut feed = ChatFeed::follow(&runtime)?;
    let id = chat(31);
    runtime.open_chat(id.clone(), acp_start()?);
    let chose = runtime.send_now(Command::Chat(ChatCommand::SetAcpMode {
        chat: id.clone(),
        mode: AcpModeId("code".to_owned()),
    }))?;
    assert_eq!(chose, Reply::Accepted);
    assert_eq!(runtime.send_now(send(&id, PROMPT))?, Reply::Accepted);
    feed.until_chat(&id, |mirror| answered(mirror, ACP_ANSWER))?;
    let state = feed.mirror(&id)?.state;
    let resume = state.resume.ok_or(NO_RESUME)?;
    let acp = state.acp.unwrap_or_default();
    assert!(acp.reported);
    assert_eq!(
        acp.modes.map(|modes| modes.current),
        Some(AcpModeId("code".to_owned()))
    );
    let closing = Command::Chat(ChatCommand::Close { chat: id.clone() });
    assert_eq!(runtime.send_now(closing)?, Reply::Accepted);
    feed.until(|event| matches!(event, EventKind::ChatClosed { .. }))?;
    let file =
        krad_store::acp_transcript_file(&id.environment, id.terminal).ok_or(NO_TRANSCRIPT)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let kept = loop {
        let kept = krad_store::read_acp_transcript(&file).unwrap_or_default();
        let done = kept.contains(&ChatEntry::Event(ChatEvent::Done));
        if done || std::time::Instant::now() > deadline {
            break kept;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    assert!(matches!(kept.first(), Some(ChatEntry::User { text, .. }) if text == PROMPT));
    let mut reopened = acp_start()?;
    reopened.session = LiveSession::Resume(resume);
    runtime.open_chat(id.clone(), reopened);
    feed.until_chat(&id, |mirror| {
        mirror.messages.len() == 2 && mirror.messages[1].text == ACP_ANSWER
    })?;
    Ok(())
}
