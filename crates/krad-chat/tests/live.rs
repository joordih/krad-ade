use krad_chat::{
    claude_conversation_file, new_session_id, spawn_live_chat, BusVars, ChatEvent, LiveConfig,
    LiveEvent, LiveSession, ResumeCut, RewindPoint, RewindRequest, RewindScope, Rewound, Runner,
    DROPS_TURN_REFUSED,
};
use krad_domain::ModelId;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

const PROGRAM: &str = "claude";
const MODEL: &str = "haiku";
const PROMPT: &str = "Reply with exactly: pong";
const EXPECTED: &str = "pong";
const RECALL_PROMPT: &str = "What exact word did you reply with before? Reply with that word only.";
const TIMEOUT: Duration = Duration::from_mins(2);
const SECOND_PROMPT: &str = "Reply with exactly: ping";
const EDITED_PROMPT: &str = "Reply with exactly: pang";
const HOME_VAR: &str = "HOME";
const EXTENSIONS: [&str; 3] = ["", ".exe", ".cmd"];

fn find(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .flat_map(|dir| EXTENSIONS.map(|extension| dir.join(format!("{program}{extension}"))))
        .find(|candidate| candidate.is_file())
}

fn answer(receiver: &Receiver<ChatEvent>) -> String {
    let mut answer = String::new();
    while let Ok(event) = receiver.recv_timeout(TIMEOUT) {
        match event {
            ChatEvent::Text(text) => answer.push_str(&text),
            ChatEvent::Tool(_)
            | ChatEvent::Edited { .. }
            | ChatEvent::Checklist(_)
            | ChatEvent::Prompted(_)
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
        system_prompt: None,
        effort: None,
        permission_mode: None,
        bus_vars: BusVars::default(),
    };
    let id = new_session_id();
    let (sender, receiver) = channel();
    let live = spawn_live_chat(&config, &LiveSession::Start(id.clone()), move |event| {
        if let LiveEvent::Chat(event) = event {
            sender.send(event).unwrap_or_default();
        }
    });
    let Ok(live) = live else {
        unreachable!("the live chat starts");
    };
    live.send(PROMPT);
    assert!(answer(&receiver).contains(EXPECTED));
    drop(live);
    let (sender, receiver) = channel();
    let resumed = spawn_live_chat(&config, &LiveSession::Resume(id), move |event| {
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

fn turn(receiver: &Receiver<LiveEvent>) -> Vec<LiveEvent> {
    let mut seen = Vec::new();
    while let Ok(event) = receiver.recv_timeout(TIMEOUT) {
        let done = matches!(
            event,
            LiveEvent::Chat(ChatEvent::Done | ChatEvent::Error(_)) | LiveEvent::Rewound(_)
        );
        seen.push(event);
        if done {
            break;
        }
    }
    seen
}

fn prompted(events: &[LiveEvent]) -> Option<RewindPoint> {
    events.iter().find_map(|event| match event {
        LiveEvent::Chat(ChatEvent::Prompted(point)) => Some(point.clone()),
        _ => None,
    })
}

#[test]
#[ignore = "runs the real claude CLI"]
fn a_live_claude_rewinds_to_an_edited_last_prompt() {
    let Some(program) = find(PROGRAM) else {
        return;
    };
    let config = LiveConfig {
        runner: Runner::Native(program),
        model: ModelId::parse(MODEL),
        cwd: std::env::temp_dir(),
        system_prompt: None,
        effort: None,
        permission_mode: None,
        bus_vars: BusVars::default(),
    };
    let spawn = |session: &LiveSession| {
        let (sender, receiver) = channel();
        let live = spawn_live_chat(&config, session, move |event| {
            sender.send(event).unwrap_or_default();
        });
        let Ok(live) = live else {
            unreachable!("the live chat starts");
        };
        (live, receiver)
    };
    let (live, receiver) = spawn(&LiveSession::Start(new_session_id()));
    live.send(PROMPT);
    let first = turn(&receiver);
    live.send(SECOND_PROMPT);
    let second = turn(&receiver);
    let Some(point) = prompted(&second) else {
        unreachable!("a sent prompt carries its uuid: {second:?}");
    };
    let RewindPoint::Message(prompt) = point.clone() else {
        unreachable!("a Claude Code prompt is a message");
    };
    assert_ne!(prompted(&first), Some(point.clone()));
    live.rewind(RewindRequest {
        point,
        scope: RewindScope::Conversation,
        home: std::env::var_os(HOME_VAR).map(PathBuf::from),
    });
    let rewound = turn(&receiver);
    let Some(LiveEvent::Rewound(Ok(Rewound::Restart(LiveSession::Cut(cut))))) = rewound.last()
    else {
        unreachable!("the last prompt is cut back: {rewound:?}");
    };
    assert_eq!(cut.drops.as_ref(), Some(&prompt));
    let cut: ResumeCut = cut.clone();
    drop(live);
    let (again, receiver) = spawn(&LiveSession::Cut(cut));
    again.send(EDITED_PROMPT);
    let edited = turn(&receiver);
    assert!(
        !edited.iter().any(|event| matches!(
            event,
            LiveEvent::Chat(ChatEvent::Error(error)) if error.contains(DROPS_TURN_REFUSED)
        )),
        "{edited:?}"
    );
    let Some(copy) = edited.iter().find_map(|event| match event {
        LiveEvent::Chat(ChatEvent::Session(id)) => Some(id.clone()),
        _ => None,
    }) else {
        unreachable!("the cut copy reports its session: {edited:?}");
    };
    let file = std::env::var_os(HOME_VAR)
        .and_then(|home| claude_conversation_file(Path::new(&home), &copy));
    let kept = file
        .and_then(|file| std::fs::read_to_string(file).ok())
        .unwrap_or_default();
    assert!(kept.contains(PROMPT), "the first exchange stays");
    assert!(!kept.contains(SECOND_PROMPT), "the edited prompt is gone");
    assert!(kept.contains(EDITED_PROMPT), "the edit takes its place");
}
