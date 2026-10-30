use krad_chat::{
    engine_subagents, new_session_id, read_engine_subagent, read_subagent_meta, spawn_engine_chat,
    AskAnswer, AskKind, BusVars, ChatEvent, EngineConfig, LiveChat, LiveEvent, LiveSession,
};
use krad_domain::{ChatEntry, GatewayAccess, GatewayKey, ModelId, PermissionMode, ResumeId};
use serde_json::{json, Value};
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

const KEY: &str = "sk-engine-agents-test";
const MODEL: &str = "gemini-3-pro";
const MODELS: &str =
    r#"{"object":"list","data":[{"id":"gemini-3-pro","object":"model","owned_by":"google"}]}"#;
const COMPLETIONS: &str = "/v1/chat/completions";
const FIRST_TASK: &str = "SUBTASK-ONE: say what the parser does";
const SECOND_TASK: &str = "SUBTASK-TWO: write the note";
const FIRST_REPORT: &str = "REPORT-ONE";
const SECOND_REPORT: &str = "REPORT-TWO";
const FINAL_ANSWER: &str = "Both done.";
const WRITE_ID: &str = "call_w";
const WAIT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(20);

type Seen = Arc<Mutex<Vec<String>>>;
type Route = fn(&str) -> String;

fn read_request(stream: &TcpStream) -> io::Result<(String, String)> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut first = String::new();
    reader.read_line(&mut first)?;
    let path = first
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_owned();
    let mut length = 0;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let line = line.trim();
        if line.is_empty() {
            break;
        }
        let (name, value) = line.split_once(':').unwrap_or((line, ""));
        if name.eq_ignore_ascii_case("content-length") {
            length = value.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    Ok((path, String::from_utf8_lossy(&body).into_owned()))
}

fn serve(mut stream: TcpStream, kind: &str, body: &str) -> io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nConnection: close\r\n\r\n{body}"
    )
}

fn fake_gateway(route: Route) -> io::Result<(String, Seen)> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let url = format!("http://{}", listener.local_addr()?);
    let seen: Seen = Arc::default();
    let kept = Arc::clone(&seen);
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let Ok((path, body)) = read_request(&stream) else {
                continue;
            };
            let served = if path == "/v1/models" {
                serve(stream, "application/json", MODELS)
            } else {
                serve(stream, "text/event-stream", &route(&body))
            };
            served.unwrap_or_default();
            if path == COMPLETIONS {
                kept.lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(body);
            }
        }
    });
    Ok((url, seen))
}

fn chunk(delta: &Value, finish: &str) -> String {
    let chunk = json!({"choices": [{"index": 0, "delta": delta, "finish_reason": finish}]});
    format!("data: {chunk}\n\ndata: [DONE]\n\n")
}

fn text(answer: &str) -> String {
    chunk(&json!({"content": answer}), "stop")
}

fn calls(calls: &[(&str, &str, Value)]) -> String {
    let calls: Vec<Value> = calls
        .iter()
        .enumerate()
        .map(|(index, (id, name, arguments))| {
            json!({"index": index, "id": id, "type": "function",
                "function": {"name": name, "arguments": arguments.to_string()}})
        })
        .collect();
    chunk(&json!({ "tool_calls": calls }), "tool_calls")
}

fn split_work(body: &str) -> String {
    if body.contains(FIRST_REPORT) && body.contains(SECOND_REPORT) {
        return text(FINAL_ANSWER);
    }
    if body.contains(FIRST_TASK) {
        return text(FIRST_REPORT);
    }
    if body.contains(SECOND_TASK) {
        if body.contains(WRITE_ID) {
            return text(SECOND_REPORT);
        }
        return calls(&[(
            WRITE_ID,
            "write",
            json!({"path": "note.txt", "content": "hi\n"}),
        )]);
    }
    calls(&[
        (
            "agent_1",
            "agent",
            json!({"description": "Explain the parser", "prompt": FIRST_TASK}),
        ),
        (
            "agent_2",
            "agent",
            json!({"description": "Write the note", "prompt": SECOND_TASK}),
        ),
    ])
}

fn write_only(body: &str) -> String {
    if body.contains(SECOND_TASK) {
        return calls(&[(
            WRITE_ID,
            "write",
            json!({"path": "note.txt", "content": "hi\n"}),
        )]);
    }
    calls(&[(
        "agent_2",
        "agent",
        json!({"description": "Write the note", "prompt": SECOND_TASK}),
    )])
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("krad-engine-agents")
        .join(format!("{name}-{}", new_session_id().as_str()));
    fs::create_dir_all(dir.join("work")).unwrap_or_default();
    dir
}

fn config(url: &str, root: &Path) -> EngineConfig {
    EngineConfig {
        access: GatewayAccess {
            url: url
                .parse()
                .unwrap_or_else(|_| unreachable!("{url} is a loopback address")),
            key: GatewayKey::parse(KEY).unwrap_or_else(|| unreachable!("the key is not blank")),
        },
        sessions: root.join("sessions"),
        home: None,
        cwd: root.join("work"),
        model: ModelId::parse(MODEL),
        system_prompt: None,
        effort: None,
        permission_mode: Some(PermissionMode::Manual),
        bus_vars: BusVars::default(),
        mcp: Vec::new(),
    }
}

fn start(config: EngineConfig, id: &ResumeId) -> (LiveChat, Receiver<LiveEvent>) {
    let (sender, events) = channel();
    let chat = spawn_engine_chat(config, &LiveSession::Start(id.clone()), move |event| {
        sender.send(event).unwrap_or_default();
    });
    (chat, events)
}

fn until(events: &Receiver<LiveEvent>, stop: impl Fn(&LiveEvent) -> bool) -> Vec<LiveEvent> {
    let mut seen = Vec::new();
    while let Ok(event) = events.recv_timeout(WAIT) {
        let done = stop(&event);
        seen.push(event);
        if done {
            break;
        }
    }
    seen
}

fn finished(event: &LiveEvent) -> bool {
    matches!(
        event,
        LiveEvent::Chat(ChatEvent::Done | ChatEvent::Error(_))
    )
}

fn all_finished(sessions: &Path, id: &ResumeId, count: usize) -> bool {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        let files = engine_subagents(sessions, id);
        let done = files
            .iter()
            .filter(|files| read_subagent_meta(&files.meta).is_ok_and(|meta| meta.finished))
            .count();
        if done == count {
            return true;
        }
        thread::sleep(POLL);
    }
    false
}

#[test]
fn two_subagents_run_in_parallel_and_their_reports_go_back_to_the_parent() -> io::Result<()> {
    let root = scratch("parallel");
    let (url, seen) = fake_gateway(split_work)?;
    let id = new_session_id();
    let (chat, events) = start(config(&url, &root), &id);
    until(&events, |event| matches!(event, LiveEvent::Ready(_)));
    chat.send("split the work");
    let asked = until(&events, |event| matches!(event, LiveEvent::Ask(_)));
    let Some(LiveEvent::Ask(ask)) = asked.last() else {
        unreachable!("the second subagent asks before writing: {asked:?}");
    };
    assert!(
        matches!(&ask.kind, AskKind::Permission { tool, .. } if tool == "Write"),
        "{:?}",
        ask.kind
    );
    let agents: Vec<&str> = asked
        .iter()
        .filter_map(|event| match event {
            LiveEvent::Chat(ChatEvent::Tool(tool)) if tool.name == "Agent" => {
                Some(tool.summary.as_str())
            }
            _ => None,
        })
        .collect();
    assert_eq!(agents, ["Explain the parser", "Write the note"]);
    chat.answer(ask, &AskAnswer::Allow);
    let answered = until(&events, finished);
    assert!(matches!(
        answered.last(),
        Some(LiveEvent::Chat(ChatEvent::Done))
    ));
    let said: String = asked
        .iter()
        .chain(&answered)
        .filter_map(|event| match event {
            LiveEvent::Chat(ChatEvent::Text(piece)) => Some(piece.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(said, FINAL_ANSWER, "subagent text stays in their cards");
    assert_eq!(
        fs::read_to_string(root.join("work").join("note.txt"))?,
        "hi\n"
    );
    drop(chat);
    let bodies = seen.lock().unwrap_or_else(PoisonError::into_inner).clone();
    let last = bodies.last().map_or("", String::as_str);
    assert!(last.contains(FIRST_REPORT) && last.contains(SECOND_REPORT));
    let sessions = root.join("sessions");
    assert!(all_finished(&sessions, &id, 2));
    let first = engine_subagents(&sessions, &id)
        .into_iter()
        .find(|files| files.record.ends_with("agent_1.jsonl"))
        .ok_or_else(|| io::Error::other("the first subagent kept no record"))?;
    let entries = read_engine_subagent(&first.record, &root.join("work"), 0)?.entries;
    assert!(matches!(entries.first(), Some(ChatEntry::User { text, .. }) if text == FIRST_TASK));
    assert!(entries.iter().any(
        |entry| matches!(entry, ChatEntry::Event(ChatEvent::Text(text)) if text == FIRST_REPORT)
    ));
    Ok(())
}

#[test]
fn interrupting_the_chat_stops_a_subagent_waiting_on_the_user() -> io::Result<()> {
    let root = scratch("interrupt");
    let (url, _) = fake_gateway(write_only)?;
    let id = new_session_id();
    let (chat, events) = start(config(&url, &root), &id);
    until(&events, |event| matches!(event, LiveEvent::Ready(_)));
    chat.send("write a note through a subagent");
    let asked = until(&events, |event| matches!(event, LiveEvent::Ask(_)));
    assert!(matches!(asked.last(), Some(LiveEvent::Ask(_))));
    chat.interrupt();
    let stopped = until(&events, finished);
    assert!(matches!(
        stopped.last(),
        Some(LiveEvent::Chat(ChatEvent::Done))
    ));
    assert!(all_finished(&root.join("sessions"), &id, 1));
    assert!(!root.join("work").join("note.txt").exists());
    Ok(())
}
