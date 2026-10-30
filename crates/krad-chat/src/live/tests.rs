use super::claude::{live_event, Routing};
use super::*;
use crate::control::AskKind;
use crate::refused::ResumeRefused;
use krad_domain::ChatEvent;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

const INIT_ID: &str = "krad_0";

fn routed(lines: &[&str], routing: &Routing) -> Vec<LiveEvent> {
    let mut events = Vec::new();
    for line in lines {
        routing.route_line(line, &mut |event| events.push(event));
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
