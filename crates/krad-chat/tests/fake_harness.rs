#![cfg(unix)]

use krad_chat::{
    new_session_id, start_chat, BusVars, ChatBackend, ChatEvent, LiveConfig, LiveEvent,
    LiveSession, Runner,
};
use krad_domain::ResumeId;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

const FAKE_HARNESSES: &str = "tests/fixtures/fake-harness";
const FAKE_CLAUDE: &str = "claude";
const FAKE_CODEX: &str = "codex";
const PROMPT: &str = "Why does it race?";
const RECORDED_ANSWER: &str = "Two tabs rotate at once.";
const RECORDED_CODEX_THREAD: &str = "019a3c5e-7b21-7d40-9c1e-5f2a8b6d4e10";
const WAIT: Duration = Duration::from_secs(10);

fn fake_harness(name: &str) -> Runner {
    Runner::Native(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(FAKE_HARNESSES)
            .join(name),
    )
}

fn config(runner: Runner) -> LiveConfig {
    LiveConfig {
        runner,
        model: None,
        cwd: std::env::temp_dir(),
        system_prompt: None,
        effort: None,
        permission_mode: None,
        bus_vars: BusVars::default(),
        usage_budget: std::sync::Arc::default(),
    }
}

fn started(backend: ChatBackend) -> (krad_chat::LiveChat, Receiver<LiveEvent>) {
    let (sender, receiver) = channel();
    let chat = start_chat(
        backend,
        &LiveSession::Start(new_session_id()),
        move |event| {
            sender.send(event).unwrap_or_default();
        },
    );
    let Ok(chat) = chat else {
        unreachable!("the fake harness starts");
    };
    (chat, receiver)
}

fn turn(receiver: &Receiver<LiveEvent>) -> Vec<ChatEvent> {
    let mut events = Vec::new();
    while let Ok(event) = receiver.recv_timeout(WAIT) {
        let LiveEvent::Chat(event) = event else {
            continue;
        };
        let done = matches!(event, ChatEvent::Done | ChatEvent::Error(_));
        events.push(event);
        if done {
            break;
        }
    }
    events
}

fn said(events: &[ChatEvent]) -> String {
    events
        .iter()
        .filter_map(|event| match event {
            ChatEvent::Text(text) | ChatEvent::TextDelta(text) => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_claude_chat_sends_a_message_and_reads_the_recorded_reply() {
    let (chat, receiver) = started(ChatBackend::Claude(config(fake_harness(FAKE_CLAUDE))));
    chat.send(PROMPT);
    let events = turn(&receiver);
    assert!(matches!(events.last(), Some(ChatEvent::Done)), "{events:?}");
    assert!(said(&events).contains(RECORDED_ANSWER), "{events:?}");
}

#[test]
fn a_codex_chat_opens_its_thread_sends_a_message_and_reads_the_recorded_reply() {
    let (chat, receiver) = started(ChatBackend::Codex(config(fake_harness(FAKE_CODEX))));
    let opening: Vec<LiveEvent> = std::iter::from_fn(|| receiver.recv_timeout(WAIT).ok())
        .take_while(|event| !matches!(event, LiveEvent::Ready(_)))
        .collect();
    let thread = ResumeId::try_from(RECORDED_CODEX_THREAD.to_owned()).ok();
    assert!(
        opening.iter().any(|event| matches!(
            event,
            LiveEvent::Chat(ChatEvent::Session(resume)) if Some(resume) == thread.as_ref()
        )),
        "the chat reports the recorded thread before it is ready"
    );
    chat.send(PROMPT);
    let events = turn(&receiver);
    assert!(matches!(events.last(), Some(ChatEvent::Done)), "{events:?}");
    assert_eq!(said(&events), RECORDED_ANSWER);
}
