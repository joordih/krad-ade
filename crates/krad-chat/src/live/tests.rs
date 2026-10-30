use super::*;
use crate::control::AskKind;
use std::path::PathBuf;

const INIT_ID: &str = "krad_0";

fn routed(lines: &[&str], pending: &AtomicUsize) -> Vec<LiveEvent> {
    let mut events = Vec::new();
    for line in lines {
        route_line(line, INIT_ID, pending, &mut |event| events.push(event));
    }
    events
}

#[test]
fn control_lines_are_routed_apart_from_the_conversation() {
    let pending = AtomicUsize::new(1);
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
        &pending,
    );
    assert_eq!(events.len(), 3, "{events:?}");
    assert!(matches!(
        &events[0],
        LiveEvent::Ask(ask) if matches!(&ask.kind, AskKind::Permission { tool, .. } if tool == "Bash")
    ));
    assert_eq!(events[1], LiveEvent::Ready(crate::Catalog::default()));
    assert_eq!(events[2], LiveEvent::Chat(ChatEvent::Text("Hi".to_owned())));
    assert_eq!(pending.load(Ordering::SeqCst), 1, "no answer closed yet");
}

#[test]
fn a_closed_answer_is_no_longer_pending_and_the_count_stays_at_zero() {
    let pending = AtomicUsize::new(1);
    let result = r#"{"type":"result","subtype":"success","result":"Hi"}"#;
    let events = routed(&[result, result], &pending);
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, LiveEvent::Chat(ChatEvent::Done)))
            .count(),
        2
    );
    assert_eq!(pending.load(Ordering::SeqCst), 0);
}

#[test]
fn a_live_config_never_prints_the_bus_token() {
    let token = "s3cr3t-bus-token";
    let config = LiveConfig {
        runner: Runner::Native(PathBuf::from("claude")),
        model: None,
        cwd: PathBuf::from("/work"),
        extra_dirs: Vec::new(),
        orchestration: Some(Orchestration {
            cli: "krad-ade-agents".to_owned(),
            env: BusVars(vec![("KRAD_ADE_TOKEN".to_owned(), token.to_owned())]),
        }),
        system_prompt: None,
        effort: None,
        permission_mode: None,
        bus_vars: BusVars(vec![("KRAD_ADE_TOKEN".to_owned(), token.to_owned())]),
        approvals: Approvals::Asked,
        gateway: None,
    };
    let printed = format!("{config:?}");
    assert!(printed.contains("KRAD_ADE_TOKEN"));
    assert!(!printed.contains(token));
}

fn gateway_config(gateway: Option<GatewayAccess>) -> LiveConfig {
    LiveConfig {
        runner: Runner::Native(PathBuf::from("claude")),
        model: ModelId::parse("gpt-5"),
        cwd: PathBuf::from("/work"),
        extra_dirs: Vec::new(),
        orchestration: None,
        system_prompt: None,
        effort: None,
        permission_mode: None,
        bus_vars: BusVars::default(),
        approvals: Approvals::Asked,
        gateway,
    }
}

fn env_of(command: &Command) -> Vec<(String, Option<String>)> {
    command
        .get_envs()
        .map(|(name, value)| {
            (
                name.to_string_lossy().into_owned(),
                value.map(|value| value.to_string_lossy().into_owned()),
            )
        })
        .collect()
}

#[test]
fn a_routed_chat_reaches_the_gateway_and_drops_the_own_login() {
    let access = GatewayAccess {
        url: "http://127.0.0.1:8317"
            .parse()
            .unwrap_or_else(|_| unreachable!("a loopback address")),
        key: krad_domain::GatewayKey::parse("sk-local").unwrap_or_else(|| unreachable!("set")),
    };
    let session = LiveSession::Start(new_session_id());
    let routed = env_of(&live_process(&gateway_config(Some(access)), &session));
    assert!(routed.contains(&(
        "ANTHROPIC_BASE_URL".to_owned(),
        Some("http://127.0.0.1:8317".to_owned())
    )));
    assert!(routed.contains(&(
        "ANTHROPIC_DEFAULT_HAIKU_MODEL".to_owned(),
        Some("gpt-5".to_owned())
    )));
    for name in CLEARED_VARS {
        assert!(routed.contains(&(name.to_owned(), None)), "{name} is kept");
    }
    let direct = env_of(&live_process(&gateway_config(None), &session));
    assert!(direct
        .iter()
        .all(|(name, _)| !name.starts_with("ANTHROPIC_")));
}

#[test]
fn messages_wait_while_the_harness_answers_and_go_in_order() {
    let mut queue = MessageQueue::default();
    assert_eq!(queue.push("first"), Some("first"));
    assert!(queue.is_busy());
    assert_eq!(queue.push("second"), None);
    assert_eq!(queue.push("third"), None);
    assert_eq!(
        queue.waiting().copied().collect::<Vec<_>>(),
        ["second", "third"]
    );
    assert_eq!(queue.finish(), Some("second"));
    assert_eq!(queue.finish(), Some("third"));
    assert!(queue.is_busy());
    assert_eq!(queue.finish(), None);
    assert!(!queue.is_busy());
    assert_eq!(queue.push("fourth"), Some("fourth"));
    queue.clear();
    assert!(!queue.is_busy());
    assert_eq!(queue.waiting().count(), 0);
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
fn a_resume_of_a_conversation_claude_code_never_kept_is_told_apart() {
    assert!(is_missing_conversation(
        "No conversation found with session ID: 4f2c0b7c-a91e"
    ));
    assert!(!is_missing_conversation("the harness exited with code 1"));
}
