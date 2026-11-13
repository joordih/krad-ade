#![cfg(unix)]

#[path = "support/chats.rs"]
mod chat_support;

use chat_support::{
    chat, claude_start, fake_runner, start_runtime, Ask as _, ChatFeed, TestResult, CLAUDE_HARNESS,
};
use krad_chat::{new_session_id, LiveSession};
use krad_domain::{
    AnswerPart, AskAnswer, AskKind, AskVerdict, AttentionKind, Prompt, ResumeId, Speaker,
};
use krad_protocol::{
    AgentRef, ChatAskId, ChatCommand, ChatMirror, Command, ErrorCode, EventKind, Reply,
};
use std::path::PathBuf;

const ASK_HARNESS: &str = "tests/fixtures/fake-harness/claude-ask";
const PROMPT: &str = "Why does it race?";
const RECORDED_ANSWER: &str = "Two tabs rotate at once.";
const RECORDED_SESSION: &str = "0b7c3a52-9f1e-4d2a-8c6b-1f2e3d4c5b6a";
const ASKED_ANSWER: &str = "Listed the folder.";
const HOME_VAR: &str = "HOME";
const RECORDED_TURN: &str = "../krad-chat/tests/fixtures/fake-harness/claude-turn.jsonl";
const PROJECTS: &str = ".claude/projects/fake-project";
const CONVERSATION_EXTENSION: &str = "jsonl";

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
    runtime.close_chat(id.clone());
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
