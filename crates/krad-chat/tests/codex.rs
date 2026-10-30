use krad_chat::{
    new_session_id, read_codex_transcript, spawn_codex_chat, Approvals, BusVars, ChatEvent,
    LiveConfig, LiveEvent, LiveSession, Runner,
};
use krad_domain::{AgentPreset, ChatEntry, PermissionMode};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

const PROGRAM: &str = "codex";
const PROMPT: &str = "Reply with exactly: pong";
const EXPECTED: &str = "pong";
const TIMEOUT: Duration = Duration::from_mins(3);
const EXTENSIONS: [&str; 3] = ["", ".exe", ".cmd"];

fn find(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .flat_map(|dir| EXTENSIONS.map(|extension| dir.join(format!("{program}{extension}"))))
        .find(|candidate| candidate.is_file())
}

fn next(receiver: &Receiver<LiveEvent>) -> Option<LiveEvent> {
    receiver.recv_timeout(TIMEOUT).ok()
}

#[test]
#[ignore = "runs the real codex CLI"]
fn a_codex_chat_opens_a_thread_answers_and_reads_back() {
    let Some(program) = find(PROGRAM) else {
        return;
    };
    let runner = Runner::Native(program);
    let config = LiveConfig {
        runner: runner.clone(),
        model: None,
        cwd: std::env::temp_dir(),
        extra_dirs: Vec::new(),
        orchestration: None,
        system_prompt: Some("Answer briefly.".to_owned()),
        effort: Some(krad_domain::Effort::Low),
        permission_mode: Some(PermissionMode::Manual),
        bus_vars: BusVars::default(),
        approvals: Approvals::Asked,
        gateway: None,
    };
    let (sender, receiver) = channel();
    let chat = spawn_codex_chat(
        config,
        &LiveSession::Start(new_session_id()),
        move |event| {
            sender.send(event).unwrap_or_default();
        },
    );
    let Some(LiveEvent::Chat(ChatEvent::Session(resume))) = next(&receiver) else {
        unreachable!("the chat reports its thread first");
    };
    let mut ready = false;
    while !ready {
        match next(&receiver) {
            Some(LiveEvent::Ready(catalog)) => {
                assert_ne!(catalog.models.len(), 0);
                ready = true;
            }
            Some(_) => {}
            None => unreachable!("codex lists what it offers"),
        }
    }
    chat.send(PROMPT);
    let mut answer = String::new();
    while let Some(event) = next(&receiver) {
        match event {
            LiveEvent::Chat(ChatEvent::Text(text)) => answer.push_str(&text),
            LiveEvent::Chat(ChatEvent::Done) => break,
            LiveEvent::Chat(ChatEvent::Error(error)) => unreachable!("codex failed: {error}"),
            _ => {}
        }
    }
    assert!(answer.to_lowercase().contains(EXPECTED));
    drop(chat);
    let history = read_codex_transcript(&runner, &std::env::temp_dir(), &resume);
    assert!(history.is_ok_and(|history| matches!(
        history.first(),
        Some(ChatEntry::User { text, preset: AgentPreset::Codex, .. }) if text == PROMPT
    )));
}
