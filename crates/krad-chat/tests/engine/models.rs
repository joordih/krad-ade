use krad_chat::{
    new_session_id, read_engine_transcript, ChatEvent, GatewayModel, LiveEvent, LiveSession, Vendor,
};
use krad_domain::{ChatEntry, ContextWindow, ModelId, ResumeId};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::PoisonError;

use super::support::{
    config, fake_gateway, finished, gateway_model, scratch, start, until, Seen, ANSWER,
    COMPLETIONS_PATH, MODEL,
};

const CLAUDE: &str = "claude-sonnet-5";
const MESSAGES_PATH: &str = "/v1/messages";
const MESSAGES_ANSWER: &str = concat!(
    r#"data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
    "\n\n",
    r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hi."}}"#,
    "\n\n",
    r#"data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":2}}"#,
    "\n\n"
);
const CUT_ANSWER: &str = concat!(
    r#"data: {"choices":[{"index":0,"delta":{"content":"Half an"},"finish_reason":"length"}]}"#,
    "\n\ndata: [DONE]\n\n"
);
const LARGE_ANSWER: &str = concat!(
    r#"data: {"choices":[{"index":0,"delta":{"content":"ok"},"finish_reason":"stop"}]}"#,
    "\n\n",
    r#"data: {"choices":[],"usage":{"prompt_tokens":120,"completion_tokens":30}}"#,
    "\n\ndata: [DONE]\n\n"
);
const COMPACT_MARK: &str = "Summarize the conversation so far";

fn paths(seen: &Seen) -> Vec<String> {
    seen.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .map(|asked| asked.path.clone())
        .collect()
}

fn one_turn(
    url: &str,
    root: &Path,
    session: &LiveSession,
    models: Vec<GatewayModel>,
) -> Vec<LiveEvent> {
    let mut claude = config(url, root);
    claude.model = ModelId::parse(CLAUDE);
    claude.models = models;
    let (chat, events) = start(claude, session);
    until(&events, |event| matches!(event, LiveEvent::Ready(_)));
    chat.send("hi");
    until(&events, finished)
}

fn session_file(root: &Path, id: &ResumeId) -> PathBuf {
    root.join("sessions")
        .join(id.as_str())
        .with_extension("jsonl")
}

#[test]
fn a_resumed_chat_speaks_the_protocol_saved_with_its_model() -> io::Result<()> {
    let root = scratch("vendor-resume");
    fs::create_dir_all(root.join("work"))?;
    let (url, seen) = fake_gateway(vec![MESSAGES_ANSWER.to_owned(), MESSAGES_ANSWER.to_owned()])?;
    let id = new_session_id();
    one_turn(
        &url,
        &root,
        &LiveSession::Start(id.clone()),
        vec![gateway_model(CLAUDE, Vendor::Anthropic)],
    );
    let resumed = one_turn(&url, &root, &LiveSession::Resume(id), Vec::new());
    assert!(matches!(
        resumed.last(),
        Some(LiveEvent::Chat(ChatEvent::Done))
    ));
    assert_eq!(paths(&seen), [MESSAGES_PATH, MESSAGES_PATH]);
    Ok(())
}

#[test]
fn a_model_saved_without_its_vendor_falls_back_to_the_offers_then_completions() -> io::Result<()> {
    let root = scratch("vendor-old");
    fs::create_dir_all(root.join("sessions"))?;
    fs::create_dir_all(root.join("work"))?;
    let old = new_session_id();
    fs::write(
        session_file(&root, &old),
        format!("{{\"kind\":\"model\",\"model\":\"{CLAUDE}\"}}\n"),
    )?;
    let (url, seen) = fake_gateway(vec![MESSAGES_ANSWER.to_owned(), ANSWER.to_owned()])?;
    one_turn(
        &url,
        &root,
        &LiveSession::Fork(old.clone()),
        vec![gateway_model(CLAUDE, Vendor::Anthropic)],
    );
    one_turn(&url, &root, &LiveSession::Resume(old), Vec::new());
    assert_eq!(paths(&seen), [MESSAGES_PATH, COMPLETIONS_PATH]);
    Ok(())
}

#[test]
fn the_context_window_of_a_model_comes_from_its_override() -> io::Result<()> {
    let root = scratch("window");
    fs::create_dir_all(root.join("work"))?;
    let (url, seen) = fake_gateway(vec![LARGE_ANSWER.to_owned()])?;
    let mut small = config(&url, &root);
    let window = ContextWindow::try_from(100).unwrap_or_default();
    small.context_windows = HashMap::from([(
        ModelId::parse(MODEL).unwrap_or_else(|| unreachable!("valid")),
        window,
    )]);
    let (chat, events) = start(small, &LiveSession::Start(new_session_id()));
    let opened = until(&events, |event| matches!(event, LiveEvent::Ready(_)));
    assert!(opened
        .iter()
        .any(|event| matches!(event, LiveEvent::Chat(ChatEvent::ContextWindow(100)))));
    chat.send("one");
    until(&events, finished);
    chat.send("two");
    until(&events, finished);
    drop(chat);
    let bodies: Vec<String> = seen
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .map(|asked| asked.body.clone())
        .collect();
    assert_eq!(bodies.len(), 3);
    assert!(!bodies[0].contains(COMPACT_MARK));
    assert!(bodies[1].contains(COMPACT_MARK), "{}", bodies[1]);
    Ok(())
}

#[test]
fn an_answer_cut_at_the_token_limit_ends_with_an_error_live_and_on_replay() -> io::Result<()> {
    let root = scratch("cut");
    fs::create_dir_all(root.join("work"))?;
    let (url, _) = fake_gateway(vec![CUT_ANSWER.to_owned()])?;
    let id = new_session_id();
    let (chat, events) = start(config(&url, &root), &LiveSession::Start(id.clone()));
    until(&events, |event| matches!(event, LiveEvent::Ready(_)));
    chat.send("explain");
    let answered = until(&events, finished);
    drop(chat);
    let cut_off =
        |event: &ChatEvent| matches!(event, ChatEvent::Error(text) if text.contains("cut off"));
    assert!(
        matches!(answered.last(), Some(LiveEvent::Chat(event)) if cut_off(event)),
        "{answered:?}"
    );
    let entries = read_engine_transcript(&root.join("sessions"), &root.join("work"), &id)?;
    assert!(matches!(entries.last(), Some(ChatEntry::Event(event)) if cut_off(event)));
    assert!(!entries.contains(&ChatEntry::Event(ChatEvent::Done)));
    Ok(())
}

#[test]
fn an_answer_cut_off_while_messages_wait_still_says_so_before_going_on() -> io::Result<()> {
    let root = scratch("cut-queued");
    fs::create_dir_all(root.join("work"))?;
    let (url, _) = fake_gateway(vec![CUT_ANSWER.to_owned(), ANSWER.to_owned()])?;
    let (chat, events) = start(config(&url, &root), &LiveSession::Start(new_session_id()));
    until(&events, |event| matches!(event, LiveEvent::Ready(_)));
    chat.send("explain");
    chat.send("and then?");
    let answered = until(&events, finished);
    assert!(
        answered
            .iter()
            .any(|event| matches!(event, LiveEvent::Notice(text) if text.contains("cut off"))),
        "{answered:?}"
    );
    assert_eq!(answered.last(), Some(&LiveEvent::Chat(ChatEvent::Done)));
    Ok(())
}
