use krad_chat::{
    new_session_id, opencode_models, read_opencode_transcript, spawn_opencode_chat, Approvals,
    BusVars, ChatEvent, LiveConfig, LiveEvent, LiveSession, OpencodeServer, Runner,
};
use krad_domain::{AgentPreset, ChatEntry, PermissionMode};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

const PROGRAM: &str = "opencode";
const PROMPT: &str = "Reply with exactly: pong";
const EXPECTED: &str = "pong";
const TIMEOUT: Duration = Duration::from_mins(2);
const EXTENSIONS: [&str; 3] = ["", ".exe", ".cmd"];

fn find(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .flat_map(|dir| EXTENSIONS.map(|extension| dir.join(format!("{program}{extension}"))))
        .find(|candidate| candidate.is_file())
}

fn config(runner: Runner) -> LiveConfig {
    LiveConfig {
        runner,
        model: None,
        cwd: std::env::temp_dir(),
        extra_dirs: Vec::new(),
        orchestration: None,
        system_prompt: Some("Answer briefly.".to_owned()),
        effort: None,
        permission_mode: Some(PermissionMode::Manual),
        bus_vars: BusVars(vec![("KRAD_TEST".to_owned(), "1".to_owned())]),
        approvals: Approvals::Asked,
        gateway: None,
    }
}

fn next(receiver: &Receiver<LiveEvent>) -> Option<LiveEvent> {
    receiver.recv_timeout(TIMEOUT).ok()
}

#[test]
#[ignore = "runs the real opencode CLI"]
fn an_opencode_chat_opens_a_session_lists_models_and_runs_a_turn() {
    let Some(program) = find(PROGRAM) else {
        return;
    };
    let runner = Runner::Native(program);
    let Ok(server) = OpencodeServer::start(&runner, &std::env::temp_dir(), &[]) else {
        unreachable!("opencode starts its server");
    };
    assert!(server.is_alive());
    let models = opencode_models(&server, None);
    assert!(models.is_ok_and(|models| !models.is_empty()));
    let (sender, receiver) = channel();
    let session = LiveSession::Start(new_session_id());
    let chat = spawn_opencode_chat(server.clone(), config(runner), &session, move |event| {
        sender.send(event).unwrap_or_default();
    });
    let Some(LiveEvent::Chat(ChatEvent::Session(resume))) = next(&receiver) else {
        unreachable!("the chat reports its session first");
    };
    assert!(resume.as_str().starts_with("ses_"));
    assert_eq!(
        next(&receiver),
        Some(LiveEvent::Chat(ChatEvent::PermissionMode(
            PermissionMode::Manual
        )))
    );
    let Some(LiveEvent::Ready(catalog)) = next(&receiver) else {
        unreachable!("the chat lists what it offers");
    };
    assert_ne!(catalog.models.len(), 0);
    chat.send(PROMPT);
    let mut answer = String::new();
    while let Some(event) = next(&receiver) {
        match event {
            LiveEvent::Chat(ChatEvent::Text(text)) => answer.push_str(&text),
            LiveEvent::Chat(ChatEvent::Done | ChatEvent::Error(_)) => break,
            _ => {}
        }
    }
    let history = read_opencode_transcript(&server, &resume);
    assert!(history.is_ok_and(|history| matches!(
        history.first(),
        Some(ChatEntry::User { text, preset: AgentPreset::Opencode, .. }) if text == PROMPT
    )));
    assert!(answer.is_empty() || answer.to_lowercase().contains(EXPECTED));
}
