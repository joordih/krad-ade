use super::*;
use krad_domain::{Effort, PermissionMode};
use std::path::PathBuf;

fn model(name: &str) -> Option<krad_domain::ModelId> {
    krad_domain::ModelId::parse(name)
}

fn live_config() -> LiveConfig {
    LiveConfig {
        runner: crate::Runner::Native(PathBuf::from("claude")),
        model: None,
        cwd: PathBuf::from("/work"),
        system_prompt: None,
        effort: None,
        permission_mode: None,
        bus_vars: BusVars::default(),
    }
}

fn session_id() -> ResumeId {
    "0b7c3a52-9f1e-4d2a-8c6b-1f2e3d4c5b6a"
        .parse()
        .unwrap_or_else(|_| unreachable!("a uuid is a valid resume id"))
}

#[test]
fn a_live_claude_reads_stream_json_and_starts_or_resumes_its_session() {
    let id = session_id();
    let started = live_chat_command(
        &LiveConfig {
            model: model("opus"),
            ..live_config()
        },
        &LiveSession::Start(id.clone()),
    );
    assert_eq!(
        started,
        [
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--model",
            "opus",
            CLAUDE_SESSION_ID,
            id.as_str(),
            CLAUDE_PERMISSION_PROMPT_TOOL,
            CLAUDE_STDIO_PROMPT_TOOL,
        ]
        .map(str::to_owned)
    );
    let resumed = live_chat_command(&live_config(), &LiveSession::Resume(id.clone()));
    assert!(resumed
        .windows(2)
        .any(|pair| pair == ["--resume".to_owned(), id.as_str().to_owned()]));
    let forked = live_chat_command(&live_config(), &LiveSession::Fork(id.clone()));
    assert!(forked
        .windows(3)
        .any(|three| three == ["--resume", id.as_str(), "--fork-session"].map(str::to_owned)));
}

#[test]
fn a_live_claude_asks_approvals_and_takes_its_prompt_and_effort_last() {
    let command = live_chat_command(
        &LiveConfig {
            system_prompt: Some("Use the bus.".to_owned()),
            effort: Some(Effort::ExtraHigh),
            ..live_config()
        },
        &LiveSession::Start(session_id()),
    );
    assert!(command.ends_with(
        &[
            "--permission-prompt-tool",
            "stdio",
            "--append-system-prompt",
            "Use the bus.",
            "--effort",
            "xhigh",
        ]
        .map(str::to_owned)
    ));
}

#[test]
fn a_live_claude_starts_in_its_permission_mode_or_in_its_own_setting() {
    let picked = live_chat_command(
        &LiveConfig {
            permission_mode: Some(PermissionMode::AcceptEdits),
            ..live_config()
        },
        &LiveSession::Resume(session_id()),
    );
    assert!(picked
        .windows(2)
        .any(|pair| pair == [CLAUDE_PERMISSION_MODE, "acceptEdits"]));
    let own = live_chat_command(&live_config(), &LiveSession::Start(session_id()));
    assert!(!own.contains(&CLAUDE_PERMISSION_MODE.to_owned()));
}

#[test]
fn a_resumed_or_forked_chat_opens_with_the_original_history() {
    let id = session_id();
    assert_eq!(LiveSession::Start(id.clone()).history(), None);
    assert_eq!(LiveSession::Resume(id.clone()).history(), Some(&id));
    assert_eq!(
        LiveSession::Fork(id.clone()).history(),
        Some(&id),
        "a copy opens with the original's history"
    );
}

#[test]
fn a_rewound_claude_resumes_a_copy_cut_at_the_kept_entry() {
    let id = session_id();
    let cut = ResumeCut {
        from: id.clone(),
        at: krad_domain::RewindId::from("a2"),
        drops: Some(krad_domain::RewindId::from("u3")),
    };
    let command = live_chat_command(&live_config(), &LiveSession::Cut(cut.clone()));
    let resumed = command
        .iter()
        .position(|arg| arg == "--resume")
        .unwrap_or_default();
    assert_eq!(
        command[resumed..resumed + 5],
        [
            "--resume",
            id.as_str(),
            "--resume-session-at=a2",
            "--resume-drops-turn=u3",
            "--fork-session",
        ]
        .map(str::to_owned)
    );
    assert_eq!(LiveSession::Cut(cut.clone()).history(), Some(&id));
    let earlier = live_chat_command(
        &live_config(),
        &LiveSession::Cut(ResumeCut { drops: None, ..cut }),
    );
    assert!(!earlier.iter().any(|arg| arg.starts_with(CLAUDE_DROPS_TURN)));
}
