use krad_chat::{
    new_session_id, spawn_live_chat, spawn_turn, Approvals, BusVars, ChatEvent, ChatRequest,
    LiveConfig, LiveEvent, LiveSession, Runner,
};
use krad_domain::{AgentPreset, ModelId};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

const PROGRAM: &str = "claude";
const MODEL: &str = "haiku";
const PROMPT: &str = "Reply with exactly: pong";
const EXPECTED: &str = "pong";
const RECALL_PROMPT: &str = "What exact word did you reply with before? Reply with that word only.";
const TIMEOUT: Duration = Duration::from_mins(2);
const EXTENSIONS: [&str; 3] = ["", ".exe", ".cmd"];

fn find(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .flat_map(|dir| EXTENSIONS.map(|extension| dir.join(format!("{program}{extension}"))))
        .find(|candidate| candidate.is_file())
}

#[test]
#[ignore = "runs the real claude CLI"]
fn claude_answers_a_turn_end_to_end() {
    let Some(program) = find(PROGRAM) else {
        return;
    };
    let (sender, receiver) = channel();
    let request = ChatRequest {
        preset: AgentPreset::ClaudeCode,
        runner: Runner::Native(program),
        model: ModelId::parse(MODEL),
        cwd: std::env::temp_dir(),
        extra_dirs: Vec::new(),
        orchestration: None,
        prompt: PROMPT.to_owned(),
    };
    let turn = spawn_turn(request, move |event| {
        sender.send(event).unwrap_or_default();
    });
    assert!(turn.is_ok(), "the turn starts");
    let mut answer = String::new();
    loop {
        match receiver.recv_timeout(TIMEOUT) {
            Ok(ChatEvent::Text(text)) => answer.push_str(&text),
            Ok(
                ChatEvent::Tool(_)
                | ChatEvent::Edited { .. }
                | ChatEvent::Thinking(_)
                | ChatEvent::Session(_)
                | ChatEvent::Model(_)
                | ChatEvent::PermissionMode(_)
                | ChatEvent::ContextTokens(_)
                | ChatEvent::ContextWindow(_),
            ) => {}
            Ok(ChatEvent::Done) => break,
            Ok(ChatEvent::Error(error)) => {
                assert!(error.is_empty(), "the turn failed: {error}");
                break;
            }
            Err(error) => {
                assert!(answer.is_empty(), "no closing event: {error}");
                break;
            }
        }
    }
    assert!(answer.contains(EXPECTED), "answer was {answer:?}");
}

fn answer(receiver: &Receiver<ChatEvent>) -> String {
    let mut answer = String::new();
    while let Ok(event) = receiver.recv_timeout(TIMEOUT) {
        match event {
            ChatEvent::Text(text) => answer.push_str(&text),
            ChatEvent::Tool(_)
            | ChatEvent::Edited { .. }
            | ChatEvent::Thinking(_)
            | ChatEvent::Session(_)
            | ChatEvent::Model(_)
            | ChatEvent::PermissionMode(_)
            | ChatEvent::ContextTokens(_)
            | ChatEvent::ContextWindow(_) => {}
            ChatEvent::Done => break,
            ChatEvent::Error(error) => {
                assert!(error.is_empty(), "the answer failed: {error}");
                break;
            }
        }
    }
    answer
}

#[test]
#[ignore = "runs the real claude CLI"]
fn a_live_claude_answers_and_resumes_its_conversation() {
    let Some(program) = find(PROGRAM) else {
        return;
    };
    let config = LiveConfig {
        runner: Runner::Native(program),
        model: ModelId::parse(MODEL),
        cwd: std::env::temp_dir(),
        extra_dirs: Vec::new(),
        orchestration: None,
        system_prompt: None,
        effort: None,
        permission_mode: None,
        bus_vars: BusVars::default(),
        approvals: Approvals::Refused,
    };
    let id = new_session_id();
    let (sender, receiver) = channel();
    let live = spawn_live_chat(
        config.clone(),
        &LiveSession::Start(id.clone()),
        move |event| {
            if let LiveEvent::Chat(event) = event {
                sender.send(event).unwrap_or_default();
            }
        },
    );
    let Ok(live) = live else {
        unreachable!("the live chat starts");
    };
    live.send(PROMPT);
    assert!(answer(&receiver).contains(EXPECTED));
    drop(live);
    let (sender, receiver) = channel();
    let resumed = spawn_live_chat(config, &LiveSession::Resume(id), move |event| {
        if let LiveEvent::Chat(event) = event {
            sender.send(event).unwrap_or_default();
        }
    });
    let Ok(resumed) = resumed else {
        unreachable!("the live chat resumes");
    };
    resumed.send(RECALL_PROMPT);
    assert!(answer(&receiver).to_lowercase().contains(EXPECTED));
}

#[test]
#[ignore = "runs the real claude CLI"]
fn claude_lists_its_models_without_a_turn() {
    let Some(program) = find(PROGRAM) else {
        return;
    };
    let models = krad_chat::claude_models(&Runner::Native(program), &std::env::temp_dir());
    assert!(models.is_ok_and(|models| !models.is_empty()));
}
