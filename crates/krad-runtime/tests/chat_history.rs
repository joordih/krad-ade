#![cfg(unix)]

#[path = "support/chats.rs"]
mod chat_support;

use chat_support::{
    chat, claude_start, fake_runner, start_runtime, Ask as _, ChatFeed, TestResult, CLAUDE_HARNESS,
};
use futures::executor::block_on;
use krad_chat::{new_session_id, LiveSession};
use krad_domain::{ChatOpener, Speaker};
use krad_protocol::{Answer, ChatCommand, ChatMirror, Command, ErrorCode, EventKind, Reply};
use std::path::PathBuf;

const HOME_VAR: &str = "HOME";
const PROJECTS: &str = ".claude/projects/fake-project";
const RECORDED_TURN: &str = "../krad-chat/tests/fixtures/fake-harness/claude-turn.jsonl";
const FIRST_QUESTION: &str = "Where does it start?";
const FIRST_ANSWER: &str = "In the reader.";
const PADDING_BYTES: usize = 2 * 1024 * 1024 + 4096;
const PAGE: usize = 64;

struct Home(PathBuf);

impl Drop for Home {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.0));
    }
}

fn line(kind: &str, role: &str, text: &str) -> String {
    format!(
        r#"{{"type":"{kind}","message":{{"role":"{role}","content":[{{"type":"text","text":"{text}"}}]}}}}"#
    )
}

fn long_conversation() -> Result<String, Box<dyn std::error::Error>> {
    let recorded =
        std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(RECORDED_TURN))?;
    let padding = format!(
        r#"{{"type":"summary","summary":"{}"}}"#,
        "x".repeat(PADDING_BYTES)
    );
    Ok([
        line("user", "user", FIRST_QUESTION),
        line("assistant", "assistant", FIRST_ANSWER),
        padding,
        recorded,
    ]
    .join("\n"))
}

#[test]
fn a_long_history_opens_at_its_tail_and_pages_in_the_start_with_its_origin() -> TestResult {
    let home = Home(std::env::temp_dir().join(format!("krad-chat-earlier-{}", std::process::id())));
    std::fs::create_dir_all(home.0.join(PROJECTS))?;
    std::env::set_var(HOME_VAR, &home.0);
    let resume = new_session_id();
    let conversation = home
        .0
        .join(PROJECTS)
        .join(format!("{}.jsonl", resume.as_str()));
    std::fs::write(conversation, long_conversation()?)?;

    let runtime = start_runtime()?;
    let mut feed = ChatFeed::follow(&runtime)?;
    let id = chat(21);
    runtime.open_chat(
        id.clone(),
        claude_start(fake_runner(CLAUDE_HARNESS), LiveSession::Resume(resume)),
    );
    feed.until(|event| matches!(event, EventKind::ChatOpened { .. }))?;
    feed.until_chat(&id, |mirror| {
        mirror.messages.len() == 2 && mirror.state.earlier
    })?;
    assert_eq!(feed.mirror(&id)?.state.origin, None);
    let show = Command::Chat(ChatCommand::ShowEarlier { chat: id.clone() });
    assert_eq!(runtime.send_now(show.clone())?, Reply::Accepted);
    feed.until_chat(&id, |mirror| !mirror.state.earlier)?;
    assert!(feed.saw(|event| matches!(event, EventKind::ChatMessagesPrepended { count: 2, .. })));
    let mut mirror = feed.mirror(&id)?;
    assert_eq!((mirror.from, mirror.len()), (2, 4));
    let request = runtime.next_request();
    let page = Command::Chat(ChatCommand::Page {
        chat: id.clone(),
        before: mirror.from,
        count: PAGE,
    });
    assert_eq!(block_on(runtime.send(request, page))?, Reply::Accepted);
    let answered = feed.until(
        |event| matches!(event, EventKind::Answered { request: answered, .. } if *answered == request),
    )?;
    let EventKind::Answered { answer, .. } = answered else {
        return Err("no answer".into());
    };
    let Answer::ChatPage(page) = *answer else {
        return Err("not a page".into());
    };
    mirror.prepend(page);
    let said: Vec<(Speaker, &str)> = mirror
        .messages
        .iter()
        .map(|message| (message.speaker, message.text.as_str()))
        .collect();
    assert_eq!(
        said[..2],
        [
            (Speaker::User, FIRST_QUESTION),
            (Speaker::Assistant, FIRST_ANSWER)
        ]
    );
    assert_eq!(ChatMirror::from(runtime.chat_snapshot(&id)?), mirror);
    let origin = mirror.state.origin.ok_or("no origin")?;
    assert_eq!(origin.opener, ChatOpener::User);
    assert!(origin.started.is_some());
    assert!(matches!(
        runtime.send_now(show.clone())?,
        Reply::Refused {
            code: ErrorCode::NotFound,
            ..
        }
    ));
    Ok(())
}
