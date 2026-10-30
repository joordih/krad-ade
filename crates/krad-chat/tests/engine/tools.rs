use krad_chat::{new_session_id, AskAnswer, AskKind, ChatEvent, LiveEvent, LiveSession, Vendor};
use krad_domain::{ModelId, PermissionMode};
use std::fs;
use std::io;
use std::thread;
use std::time::Duration;

use super::support::{
    call_stream, completions, config, fake_gateway, fake_page, finished, gateway_model, scratch,
    start, until, ANSWER,
};

const PAGE: &str = "<html><body><h1>Changelog</h1><p>serde 1.0.300 adds flatten fixes</p><p>Unrelated footer text</p></body></html>";
const PAGE_PROMPT: &str = "Which serde version is the latest?";
const EXTRACT: &str = "The latest is serde 1.0.300.";

#[test]
fn a_web_page_is_fetched_after_the_user_allows_it_and_read_as_text() -> io::Result<()> {
    let root = scratch("fetch");
    fs::create_dir_all(root.join("work"))?;
    let page = fake_page(
        "<html><head><script>var x = 1;</script></head><body><h1>Release notes</h1><p>Hello from the page &amp; more</p></body></html>",
    )?;
    let fetch = call_stream("call_fetch", "web_fetch", &serde_json::json!({"url": page}));
    let (url, seen) = fake_gateway(vec![fetch, ANSWER.to_owned()])?;
    let (chat, events) = start(config(&url, &root), &LiveSession::Start(new_session_id()));
    until(&events, |event| matches!(event, LiveEvent::Ready(_)));
    chat.send("read the release notes");
    let asked = until(&events, |event| matches!(event, LiveEvent::Ask(_)));
    let Some(LiveEvent::Ask(ask)) = asked.last() else {
        unreachable!("fetching from a local host asks in Manual mode: {asked:?}");
    };
    let AskKind::Permission { tool, summary, .. } = &ask.kind else {
        unreachable!("a fetch asks for permission: {:?}", ask.kind);
    };
    assert_eq!(
        (tool.as_str(), summary.as_str()),
        ("WebFetch", page.as_str())
    );
    assert!(ask.can_allow_always(), "a permission can be remembered");
    chat.answer(ask, &AskAnswer::Allow);
    let answered = until(&events, finished);
    assert!(matches!(
        answered.last(),
        Some(LiveEvent::Chat(ChatEvent::Done))
    ));
    drop(chat);
    let bodies = completions(&seen);
    let last = bodies.last().map_or("", String::as_str);
    assert!(last.contains("# Release notes"), "{last}");
    assert!(last.contains("Hello from the page & more"), "{last}");
    assert!(!last.contains("var x"), "{last}");
    Ok(())
}

#[cfg(unix)]
#[test]
fn background_shells_die_with_their_chat() -> io::Result<()> {
    let root = scratch("background");
    fs::create_dir_all(root.join("work"))?;
    let start_call = call_stream(
        "call_bg",
        "shell",
        &serde_json::json!({"command": "sleep 1 && touch late.txt", "run_in_background": true}),
    );
    let (url, seen) = fake_gateway(vec![start_call, ANSWER.to_owned()])?;
    let mut bypass = config(&url, &root);
    bypass.permission_mode = Some(PermissionMode::BypassPermissions);
    let (chat, events) = start(bypass, &LiveSession::Start(new_session_id()));
    until(&events, |event| matches!(event, LiveEvent::Ready(_)));
    chat.send("start it");
    until(&events, finished);
    let bodies = completions(&seen);
    assert!(bodies.last().is_some_and(|body| body.contains("id bg1")));
    drop(chat);
    thread::sleep(Duration::from_secs(2));
    assert!(!root.join("work").join("late.txt").exists());
    Ok(())
}

#[test]
fn web_search_sends_a_side_request_with_only_native_search() -> io::Result<()> {
    let root = scratch("search");
    fs::create_dir_all(root.join("work"))?;
    let search = call_stream(
        "call_s",
        "web_search",
        &serde_json::json!({"query": "latest serde"}),
    );
    let found = concat!(
        r#"data: {"choices":[{"index":0,"delta":{"content":"serde 1.0.300"},"finish_reason":"stop"}]}"#,
        "\n\ndata: [DONE]\n\n"
    );
    let (url, seen) = fake_gateway(vec![search, found.to_owned(), ANSWER.to_owned()])?;
    let (chat, events) = start(config(&url, &root), &LiveSession::Start(new_session_id()));
    until(&events, |event| matches!(event, LiveEvent::Ready(_)));
    chat.send("which serde?");
    let answered = until(&events, finished);
    assert!(!answered
        .iter()
        .any(|event| matches!(event, LiveEvent::Ask(_))));
    drop(chat);
    let bodies = completions(&seen);
    assert_eq!(bodies.len(), 3);
    let side: serde_json::Value = serde_json::from_str(&bodies[1])?;
    assert_eq!(side["tools"], serde_json::json!([{"google_search": {}}]));
    assert!(bodies[1].contains("latest serde"));
    assert!(bodies[2].contains("serde 1.0.300"));
    Ok(())
}

#[test]
fn a_fetch_with_a_prompt_returns_the_models_extract_of_the_page() -> io::Result<()> {
    let root = scratch("fetch-prompt");
    fs::create_dir_all(root.join("work"))?;
    let page = fake_page(PAGE)?;
    let fetch = call_stream(
        "call_fetch",
        "web_fetch",
        &serde_json::json!({"url": page, "prompt": PAGE_PROMPT}),
    );
    let extract = format!(
        "data: {}\n\ndata: [DONE]\n\n",
        serde_json::json!({"choices": [{"index": 0, "delta": {"content": EXTRACT}, "finish_reason": "stop"}]})
    );
    let (url, seen) = fake_gateway(vec![fetch, extract, ANSWER.to_owned()])?;
    let mut bypass = config(&url, &root);
    bypass.permission_mode = Some(PermissionMode::BypassPermissions);
    let (chat, events) = start(bypass, &LiveSession::Start(new_session_id()));
    until(&events, |event| matches!(event, LiveEvent::Ready(_)));
    chat.send("which serde is the latest?");
    until(&events, finished);
    drop(chat);
    let bodies = completions(&seen);
    assert_eq!(bodies.len(), 3);
    let side: serde_json::Value = serde_json::from_str(&bodies[1])?;
    let asked = side["messages"][1]["content"].as_str().unwrap_or_default();
    assert!(asked.contains(PAGE_PROMPT), "{asked}");
    assert!(
        asked.contains("serde 1.0.300 adds flatten fixes"),
        "{asked}"
    );
    assert!(asked.contains(&page), "{asked}");
    assert_eq!(side["tools"], serde_json::Value::Null);
    let main: serde_json::Value = serde_json::from_str(&bodies[2])?;
    let result = main["messages"]
        .as_array()
        .and_then(|turns| turns.last())
        .and_then(|turn| turn["content"].as_str());
    assert_eq!(result, Some(format!("{page}\n\n{EXTRACT}").as_str()));
    Ok(())
}

#[test]
fn web_search_on_a_model_without_native_search_says_so_without_a_side_request() -> io::Result<()> {
    const LLAMA: &str = "llama-4-maverick";
    let root = scratch("search-other");
    fs::create_dir_all(root.join("work"))?;
    let search = call_stream(
        "call_s",
        "web_search",
        &serde_json::json!({"query": "latest serde"}),
    );
    let (url, seen) = fake_gateway(vec![search, ANSWER.to_owned()])?;
    let mut other = config(&url, &root);
    other.model = ModelId::parse(LLAMA);
    other.models = vec![gateway_model(LLAMA, Vendor::Other)];
    let (chat, events) = start(other, &LiveSession::Start(new_session_id()));
    until(&events, |event| matches!(event, LiveEvent::Ready(_)));
    chat.send("which serde?");
    until(&events, finished);
    drop(chat);
    let bodies = completions(&seen);
    assert_eq!(bodies.len(), 2, "no side request goes out");
    assert!(
        bodies[1].contains(&format!("Web search is not available for {LLAMA}")),
        "{}",
        bodies[1]
    );
    Ok(())
}
