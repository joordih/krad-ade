use krad_chat::{new_session_id, AskAnswer, AskKind, ChatEvent, LiveEvent, LiveSession};
use krad_domain::PermissionMode;
use std::fs;
use std::io;
use std::thread;
use std::time::Duration;

use super::support::{
    call_stream, completions, config, fake_gateway, fake_page, finished, scratch, start, until,
    ANSWER,
};

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
