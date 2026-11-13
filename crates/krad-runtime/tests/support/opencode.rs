use serde_json::{json, Value};
use std::collections::HashMap;
use std::error::Error;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

pub const ANSWER: &str = "pong";
pub const MODEL: &str = "opencode/pickle";
pub const COMMAND: &str = "review";
const STATE_HOME_VAR: &str = "XDG_STATE_HOME";
const SERVICE_FILE: [&str; 2] = ["opencode", "service.json"];
const SESSIONS: &str = "/api/session";
const SUBSCRIBER_WAIT: Duration = Duration::from_secs(5);
const SUBSCRIBER_POLL: Duration = Duration::from_millis(20);
const EVENTS_DELAY: Duration = Duration::from_millis(50);

#[derive(Default)]
struct Served {
    sessions: HashMap<String, Vec<Value>>,
    subscribers: Vec<TcpStream>,
    prompts: usize,
}

type Shared = Arc<Mutex<Served>>;

struct Asked {
    method: String,
    path: String,
    body: Value,
}

fn read_request(stream: &TcpStream) -> std::io::Result<Asked> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut first = String::new();
    reader.read_line(&mut first)?;
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let target = parts.next().unwrap_or_default();
    let path = target.split('?').next().unwrap_or_default().to_owned();
    let mut length = 0;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let line = line.trim();
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    Ok(Asked {
        method,
        path,
        body: serde_json::from_slice(&body).unwrap_or(Value::Null),
    })
}

fn answer(mut stream: TcpStream, status: &str, body: &Value) {
    let text = body.to_string();
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
        text.len()
    )
    .unwrap_or_default();
}

fn ok(stream: TcpStream, data: &Value) {
    answer(stream, "200 OK", &json!({ "data": data }));
}

fn not_found(stream: TcpStream) {
    answer(stream, "404 Not Found", &json!({}));
}

fn broadcast(served: &Shared, events: &[Value]) {
    let mut served = served.lock().unwrap_or_else(PoisonError::into_inner);
    for event in events {
        let line = format!("data: {event}\n\n");
        served
            .subscribers
            .retain_mut(|subscriber| subscriber.write_all(line.as_bytes()).is_ok());
    }
}

fn wait_for_subscriber(served: &Shared) {
    let deadline = Instant::now() + SUBSCRIBER_WAIT;
    while Instant::now() < deadline {
        if !served
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .subscribers
            .is_empty()
        {
            return;
        }
        thread::sleep(SUBSCRIBER_POLL);
    }
}

fn prompted(served: &Shared, session: &str, text: &str) -> String {
    let id = {
        let mut held = served.lock().unwrap_or_else(PoisonError::into_inner);
        held.prompts += 1;
        let id = format!("msg_{}", held.prompts);
        let messages = held.sessions.entry(session.to_owned()).or_default();
        messages.push(json!({"type": "user", "id": id, "text": text}));
        messages.push(json!({"type": "assistant", "content": [{"type": "text", "text": ANSWER}]}));
        id
    };
    let served = Arc::clone(served);
    let session = session.to_owned();
    thread::spawn(move || {
        wait_for_subscriber(&served);
        thread::sleep(EVENTS_DELAY);
        broadcast(
            &served,
            &[
                json!({"type": "session.text.ended", "data": {"sessionID": session, "text": ANSWER}}),
                json!({"type": "session.execution.succeeded", "data": {"sessionID": session}}),
            ],
        );
    });
    id
}

fn session_route(served: &Shared, asked: &Asked, stream: TcpStream) {
    let rest = asked
        .path
        .trim_start_matches(SESSIONS)
        .trim_start_matches('/');
    let (session, tail) = rest.split_once('/').unwrap_or((rest, ""));
    let known = served
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .sessions
        .get(session)
        .cloned();
    match (asked.method.as_str(), tail, known) {
        ("GET", "", Some(_)) => ok(stream, &json!({ "id": session })),
        ("GET", "message", Some(messages)) => {
            answer(stream, "200 OK", &json!({"data": messages, "cursor": {}}));
        }
        ("POST", "prompt", Some(_)) => {
            let text = asked.body["text"].as_str().unwrap_or_default();
            let id = prompted(served, session, text);
            ok(stream, &json!({ "id": id }));
        }
        (_, _, Some(_)) => ok(stream, &json!({})),
        (_, _, None) => not_found(stream),
    }
}

fn route(served: &Shared, asked: &Asked, stream: TcpStream) {
    match (asked.method.as_str(), asked.path.as_str()) {
        ("GET", "/api/info") => answer(
            stream,
            "200 OK",
            &json!({"version": "9.9.9", "pid": std::process::id()}),
        ),
        ("GET", "/api/event") => {
            let mut stream = stream;
            if stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n")
                .is_ok()
            {
                served
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .subscribers
                    .push(stream);
            }
        }
        ("GET", "/api/model") => ok(
            stream,
            &json!([{"id": "pickle", "providerID": "opencode", "name": "Pickle", "limit": {"context": 1000}}]),
        ),
        ("GET", "/api/model/default") => ok(stream, &Value::Null),
        ("GET", "/api/command") => ok(
            stream,
            &json!([{"name": COMMAND, "description": "Review the changes"}]),
        ),
        ("GET", "/api/provider" | "/api/skill") => ok(stream, &json!([])),
        ("POST", SESSIONS) => {
            let id = asked.body["id"].as_str().unwrap_or_default().to_owned();
            served
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .sessions
                .insert(id.clone(), Vec::new());
            ok(stream, &json!({ "id": id }));
        }
        (_, path) if path.starts_with(SESSIONS) => session_route(served, asked, stream),
        _ => not_found(stream),
    }
}

pub fn serve() -> Result<String, Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let url = format!("http://{}", listener.local_addr()?);
    let served = Shared::default();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let served = Arc::clone(&served);
            thread::spawn(move || {
                if let Ok(asked) = read_request(&stream) {
                    route(&served, &asked, stream);
                }
            });
        }
    });
    Ok(url)
}

pub fn register(state_home: &Path, url: &str) -> Result<PathBuf, Box<dyn Error>> {
    let file = SERVICE_FILE
        .iter()
        .fold(state_home.to_path_buf(), |path, part| path.join(part));
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let registration = json!({"url": url, "pid": std::process::id(), "version": "9.9.9"});
    std::fs::write(&file, registration.to_string())?;
    std::env::set_var(STATE_HOME_VAR, state_home);
    Ok(file)
}
