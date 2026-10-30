use krad_chat::{
    new_session_id, start_chat, BusVars, ChatBackend, ChatEvent, EngineConfig, GatewayModel,
    LiveChat, LiveEvent, LiveSession, ModelOffer, Vendor,
};
use krad_domain::{GatewayAccess, GatewayKey, ModelId, PermissionMode};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::Duration;

pub(crate) const KEY: &str = "sk-engine-test";
pub(crate) const MODEL: &str = "gemini-3-pro";
pub(crate) const COMPLETIONS_PATH: &str = "/v1/chat/completions";
pub(crate) const WRITE_CALL: &str = concat!(
    r#"data: {"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"write","arguments":"{\"path\":\"note.txt\",\"content\":\"hi\\n\"}"}}]},"finish_reason":null}]}"#,
    "\n\n",
    r#"data: {"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}"#,
    "\n\n",
    r#"data: {"choices":[],"usage":{"prompt_tokens":120,"completion_tokens":30}}"#,
    "\n\ndata: [DONE]\n\n"
);
pub(crate) const ANSWER: &str = concat!(
    r#"data: {"choices":[{"index":0,"delta":{"content":"Wrote "},"finish_reason":null}]}"#,
    "\n\n",
    r#"data: {"choices":[{"index":0,"delta":{"content":"it."},"finish_reason":"stop"}]}"#,
    "\n\ndata: [DONE]\n\n"
);
pub(crate) const QUESTION_CALL: &str = concat!(
    r#"data: {"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_q","type":"function","function":{"name":"ask_user","arguments":"{\"questions\":[{\"question\":\"Which sort?\",\"header\":\"Sort\",\"options\":[{\"label\":\"Quicksort\"},{\"label\":\"Merge sort\"}]}]}"}}]},"finish_reason":"tool_calls"}]}"#,
    "\n\ndata: [DONE]\n\n"
);
pub(crate) const WAIT: Duration = Duration::from_secs(10);

pub(crate) struct Asked {
    pub(crate) path: String,
    pub(crate) authorization: Option<String>,
    pub(crate) body: String,
}

pub(crate) fn read_request(stream: &TcpStream) -> io::Result<Asked> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut first = String::new();
    reader.read_line(&mut first)?;
    let path = first
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_owned();
    let (mut length, mut authorization) = (0, None);
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
        if name.eq_ignore_ascii_case("authorization") {
            authorization = Some(value.trim().to_owned());
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    Ok(Asked {
        path,
        authorization,
        body: String::from_utf8_lossy(&body).into_owned(),
    })
}

pub(crate) fn answer(mut stream: TcpStream, kind: &str, body: &str) -> io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nConnection: close\r\n\r\n{body}"
    )
}

pub(crate) type Seen = Arc<Mutex<Vec<Asked>>>;

pub(crate) fn streams(bodies: &[&str]) -> Vec<String> {
    bodies.iter().map(|body| (*body).to_owned()).collect()
}

pub(crate) fn call_stream(id: &str, name: &str, arguments: &serde_json::Value) -> String {
    let chunk = serde_json::json!({"choices": [{"index": 0, "delta": {"tool_calls": [{
        "index": 0,
        "id": id,
        "type": "function",
        "function": {"name": name, "arguments": arguments.to_string()}
    }]}, "finish_reason": "tool_calls"}]});
    format!("data: {chunk}\n\ndata: [DONE]\n\n")
}

pub(crate) fn fake_page(html: &'static str) -> io::Result<String> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let url = format!("http://{}/page", listener.local_addr()?);
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream.try_clone().unwrap_or_else(|_| unreachable!()));
            let mut line = String::new();
            while reader.read_line(&mut line).is_ok_and(|read| read > 2) {
                line.clear();
            }
            answer(stream, "text/html; charset=utf-8", html).unwrap_or_default();
        }
    });
    Ok(url)
}

pub(crate) fn serve_gateway(
    mut reply: impl FnMut(&Asked) -> String + Send + 'static,
) -> io::Result<(String, Seen)> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let url = format!("http://{}", listener.local_addr()?);
    let seen: Seen = Arc::default();
    let kept = Arc::clone(&seen);
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let Ok(asked) = read_request(&stream) else {
                continue;
            };
            answer(stream, "text/event-stream", &reply(&asked)).unwrap_or_default();
            kept.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(asked);
        }
    });
    Ok((url, seen))
}

pub(crate) fn fake_gateway(streams: Vec<String>) -> io::Result<(String, Seen)> {
    let mut streams: VecDeque<String> = streams.into();
    serve_gateway(move |_| streams.pop_front().unwrap_or_else(|| ANSWER.to_owned()))
}

pub(crate) fn gateway_model(id: &str, vendor: Vendor) -> GatewayModel {
    GatewayModel {
        offer: ModelOffer {
            value: id.to_owned(),
            name: id.to_owned(),
            description: None,
            efforts: Vec::new(),
            thinking_off: false,
            source: None,
        },
        vendor,
    }
}

pub(crate) fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("krad-engine-e2e")
        .join(format!("{name}-{}", new_session_id().as_str()));
    fs::create_dir_all(&dir).unwrap_or_default();
    dir
}

pub(crate) fn config(url: &str, root: &Path) -> EngineConfig {
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
        models: vec![gateway_model(MODEL, Vendor::Google)],
        context_windows: HashMap::new(),
        system_prompt: None,
        effort: None,
        permission_mode: Some(PermissionMode::Manual),
        bus_vars: BusVars::default(),
        mcp: Vec::new(),
    }
}

pub(crate) fn finished(event: &LiveEvent) -> bool {
    matches!(
        event,
        LiveEvent::Chat(ChatEvent::Done | ChatEvent::Error(_))
    )
}

pub(crate) fn completions(seen: &Seen) -> Vec<String> {
    seen.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .filter(|asked| asked.path == COMPLETIONS_PATH)
        .map(|asked| asked.body.clone())
        .collect()
}

pub(crate) fn start(
    config: EngineConfig,
    session: &LiveSession,
) -> (LiveChat, Receiver<LiveEvent>) {
    let (sender, events) = channel();
    let chat = start_chat(ChatBackend::Gateway(config), session, move |event| {
        sender.send(event).unwrap_or_default();
    })
    .unwrap_or_else(|_| unreachable!("a gateway chat always starts"));
    (chat, events)
}

pub(crate) fn until(
    events: &Receiver<LiveEvent>,
    stop: impl Fn(&LiveEvent) -> bool,
) -> Vec<LiveEvent> {
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
