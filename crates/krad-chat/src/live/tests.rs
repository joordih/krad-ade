use super::claude::{live_event, Routing};
use super::*;
use crate::cloud::LineOrigin;
use crate::control::AskKind;
use crate::refused::ResumeRefused;
use krad_domain::ChatEvent;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

const INIT_ID: &str = "krad_0";

fn routed(lines: &[&str], routing: &Routing) -> Vec<LiveEvent> {
    let mut events = Vec::new();
    for line in lines {
        routing.route_line(line, LineOrigin::Live, &mut |event| events.push(event));
    }
    events
}

#[test]
fn control_lines_are_routed_apart_from_the_conversation() {
    let routing = Routing::new(INIT_ID, 1);
    let events = routed(
        &[
            r#"{"type":"control_request","request_id":"r1","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"ls"}}}"#,
            r#"{"type":"control_response","response":{"subtype":"success","request_id":"krad_0","response":{"models":[]}}}"#,
            r#"{"type":"control_response","response":{"subtype":"success","request_id":"krad_1","response":{}}}"#,
            r#"{"type":"control_request","request_id":"r2","request":{"subtype":"hook_callback"}}"#,
            r#"{"type":"control_request","request":{"subtype":"can_use_tool"}}"#,
            "not json",
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Hi"}]}}"#,
        ],
        &routing,
    );
    assert_eq!(events.len(), 3, "{events:?}");
    assert!(matches!(
        &events[0],
        LiveEvent::Ask(ask) if matches!(&ask.kind, AskKind::Permission { tool, .. } if tool == "Bash")
    ));
    assert_eq!(events[1], LiveEvent::Ready(crate::Catalog::default()));
    assert_eq!(events[2], LiveEvent::Chat(ChatEvent::Text("Hi".to_owned())));
    assert_eq!(
        routing.pending.load(Ordering::SeqCst),
        1,
        "no answer closed yet"
    );
}

#[test]
fn a_closed_answer_is_no_longer_pending_and_the_count_stays_at_zero() {
    let routing = Routing::new(INIT_ID, 1);
    let result = r#"{"type":"result","subtype":"success","result":"Hi"}"#;
    let events = routed(&[result, result], &routing);
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, LiveEvent::Chat(ChatEvent::Done)))
            .count(),
        2
    );
    assert_eq!(routing.pending.load(Ordering::SeqCst), 0);
}

#[test]
fn a_turn_claude_code_opens_by_itself_is_pending_until_it_closes() {
    let routing = Routing::new(INIT_ID, 0);
    let init =
        r#"{"type":"system","subtype":"init","session_id":"0b7c3a52-9f1e-4d2a-8c6b-1f2e3d4c5b6a"}"#;
    routed(&[init], &routing);
    assert_eq!(routing.pending.load(Ordering::SeqCst), 1);
    routed(
        &[r#"{"type":"result","subtype":"success","result":"Hi"}"#],
        &routing,
    );
    assert_eq!(routing.pending.load(Ordering::SeqCst), 0);
}

#[test]
fn a_turn_started_for_a_sent_message_is_not_counted_twice() {
    let routing = Routing::new(INIT_ID, 1);
    routed(&[r#"{"type":"system","subtype":"init"}"#], &routing);
    assert_eq!(routing.pending.load(Ordering::SeqCst), 1);
}

#[test]
fn a_live_config_never_prints_the_bus_token() {
    let token = "s3cr3t-bus-token";
    let config = LiveConfig {
        runner: Runner::Native(PathBuf::from("claude")),
        model: None,
        cwd: PathBuf::from("/work"),
        system_prompt: None,
        effort: None,
        permission_mode: None,
        bus_vars: BusVars(vec![("KRAD_ADE_TOKEN".to_owned(), token.to_owned())]),
        usage_budget: std::sync::Arc::default(),
    };
    let printed = format!("{config:?}");
    assert!(printed.contains("KRAD_ADE_TOKEN"));
    assert!(!printed.contains(token));
}

#[test]
fn session_ids_are_distinct_version_4_uuids() {
    let (first, second) = (new_session_id(), new_session_id());
    assert_ne!(first, second);
    let id = first.as_str();
    let groups: Vec<usize> = id.split('-').map(str::len).collect();
    assert_eq!(groups, [8, 4, 4, 4, 12]);
    assert_eq!(id.chars().nth(14), Some('4'));
    assert!(id.chars().nth(19).is_some_and(|ch| "89ab".contains(ch)));
}

#[test]
fn only_the_stderr_tail_is_kept() {
    let tail = Mutex::new(String::new());
    let noisy = "x".repeat(STDERR_TAIL_CHARS * 3);
    keep_stderr_tail(format!("{noisy}\nlast line\n").as_bytes(), &tail);
    let kept = tail.into_inner().unwrap_or_default();
    assert!(kept.ends_with("last line\n"));
    assert!(kept.chars().count() <= STDERR_TAIL_CHARS + 1);
}

#[test]
fn claude_code_refusals_to_resume_are_told_apart_from_other_failures() {
    let error = |text: &str| live_event(ChatEvent::Error(text.to_owned()));
    let missing = "No conversation found with session ID: 4f2c0b7c-a91e";
    assert_eq!(
        error(missing),
        LiveEvent::Refused(ResumeRefused::MissingConversation(missing.to_owned()))
    );
    let kept = "Resume rejected by --resume-drops-turn: range";
    assert_eq!(
        error(kept),
        LiveEvent::Refused(ResumeRefused::TurnNotDropped(kept.to_owned()))
    );
    assert_eq!(
        error("the harness exited with code 1"),
        LiveEvent::Chat(ChatEvent::Error(
            "the harness exited with code 1".to_owned()
        ))
    );
}

#[test]
fn a_usage_reply_reaches_the_chat_as_its_plan_usage_and_a_failed_one_is_left_out() {
    let routing = Routing::new(INIT_ID, 0);
    let events = routed(
        &[
            r#"{"type":"control_response","response":{"subtype":"success","request_id":"krad_usage_0","response":{"subscription_type":"pro","rate_limits":{"limits":[{"kind":"session","group":"session","percent":12,"resets_at":null,"severity":"normal","is_active":true}]}}}}"#,
            r#"{"type":"control_response","response":{"subtype":"error","request_id":"krad_usage_1","error":"Unknown subtype"}}"#,
            r#"{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","resetsAt":1794249000},"uuid":"u","session_id":"s"}"#,
        ],
        &routing,
    );
    let [LiveEvent::Usage(read), LiveEvent::Limit(Some(alert))] = events.as_slice() else {
        unreachable!("a reading and an alert: {events:?}");
    };
    assert_eq!(read.plan.as_deref(), Some("pro"));
    assert_eq!(read.windows.len(), 1);
    assert_eq!(alert.status, krad_domain::LimitStatus::Reached);
}

#[test]
fn a_replayed_claude_journal_shows_only_the_asks_still_pending() {
    let routing = Routing::new(INIT_ID, 0);
    let mut events = Vec::new();
    for line in [
        r#"{"type":"control_response","response":{"subtype":"success","request_id":"krad_0","response":{"models":[]}}}"#,
        r#"{"type":"assistant","message":{"content":[{"type":"text","text":"old"}]}}"#,
        r#"{"type":"control_request","request_id":"r9","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"rm -rf build"}}}"#,
        r#"{"type":"result","subtype":"success","result":"old"}"#,
    ] {
        routing.route_line(line, LineOrigin::Replayed, &mut |event| events.push(event));
    }
    assert!(
        matches!(
            events.as_slice(),
            [LiveEvent::Ask(ask)] if matches!(&ask.kind, AskKind::Permission { tool, .. } if tool == "Bash")
        ),
        "{events:?}"
    );
    assert_eq!(routing.pending.load(Ordering::SeqCst), 0);
    routing.route_line(
        r#"{"type":"assistant","message":{"content":[{"type":"text","text":"new"}]}}"#,
        LineOrigin::Live,
        &mut |event| events.push(event),
    );
    assert_eq!(
        events.last(),
        Some(&LiveEvent::Chat(ChatEvent::Text("new".to_owned())))
    );
}
