use krad_chat::{new_session_id, BusVars, EngineConfig, GatewayModel, ModelOffer, Vendor};
use krad_domain::{GatewayAccess, GatewayKey, ModelId, PermissionMode};
use std::collections::HashMap;
use std::error::Error;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};

pub const KEY: &str = "sk-runtime-gateway-test";
const MODEL: &str = "gemini-3-pro";
pub const ANSWERED: &str = "Wrote it.";
pub const ANSWER: &str = concat!(
    r#"data: {"choices":[{"index":0,"delta":{"content":"Wrote "},"finish_reason":null}]}"#,
    "\n\n",
    r#"data: {"choices":[{"index":0,"delta":{"content":"it."},"finish_reason":"stop"}]}"#,
    "\n\ndata: [DONE]\n\n"
);
const NO_KEY: &str = "the test key is blank";

fn drain_request(stream: &TcpStream) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
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
    reader.read_exact(&mut body)
}

pub fn serve(stream: &'static str) -> Result<String, Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let url = format!("http://{}", listener.local_addr()?);
    std::thread::spawn(move || {
        for mut asked in listener.incoming().flatten() {
            if drain_request(&asked).is_ok() {
                write!(
                    asked,
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n{stream}"
                )
                .unwrap_or_default();
            }
        }
    });
    Ok(url)
}

pub fn scratch() -> Result<PathBuf, Box<dyn Error>> {
    let root = std::env::temp_dir().join(format!(
        "krad-runtime-gateway-{}",
        new_session_id().as_str()
    ));
    std::fs::create_dir_all(root.join("work"))?;
    Ok(root)
}

pub fn access(url: &str) -> Result<GatewayAccess, Box<dyn Error>> {
    Ok(GatewayAccess {
        url: url.parse()?,
        key: GatewayKey::parse(KEY).ok_or(NO_KEY)?,
    })
}

pub fn config(root: &Path) -> EngineConfig {
    EngineConfig {
        host: krad_host::HostLink::Local,
        sessions: root.join("sessions"),
        home: None,
        cwd: root.join("work"),
        model: ModelId::parse(MODEL),
        models: vec![GatewayModel {
            offer: ModelOffer {
                value: MODEL.to_owned(),
                name: MODEL.to_owned(),
                ..ModelOffer::default()
            },
            vendor: Vendor::Google,
        }],
        context_windows: HashMap::new(),
        system_prompt: None,
        effort: None,
        permission_mode: Some(PermissionMode::Manual),
        bus_vars: BusVars::default(),
        mcp: Vec::new(),
        skills: krad_domain::SkillSettings::default(),
    }
}
