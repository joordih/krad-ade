mod examples;

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::path::PathBuf;

use crate::command::{Command, ErrorCode, Reply};
use crate::event::{Feed, Scope};
use crate::frame::{FrameKind, Message, LENGTH_BYTES, MAX_FRAME_BYTES, TERMINAL_ID_BYTES};
use crate::ids::ProtocolVersion;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const SCHEMA_FILE: &str = "../../sdk/schema/krad-runtime-protocol.json";
const WRITE_VAR: &str = "KRAD_PROTOCOL_SCHEMA";
const WRITE_VALUE: &str = "write";

const ERROR_CODES: [ErrorCode; 8] = [
    ErrorCode::Unsupported,
    ErrorCode::NotFound,
    ErrorCode::AlreadyAnswered,
    ErrorCode::Invalid,
    ErrorCode::Unavailable,
    ErrorCode::Busy,
    ErrorCode::Forbidden,
    ErrorCode::Failed,
];

const SCOPES: [Scope; 1] = [Scope::Runtime];

const AREAS: [&str; 29] = [
    "runtime",
    "bus",
    "events",
    "attention",
    "usage",
    "models",
    "health",
    "opencode",
    "environments",
    "session",
    "group",
    "node",
    "terminal",
    "host",
    "chat",
    "agent",
    "gateway",
    "settings",
    "git",
    "changes",
    "kraddie",
    "variant",
    "review",
    "code_host",
    "runner",
    "cloud",
    "repo",
    "file",
    "editor",
];

const fn listed_code(code: ErrorCode) -> bool {
    match code {
        ErrorCode::Unsupported
        | ErrorCode::NotFound
        | ErrorCode::AlreadyAnswered
        | ErrorCode::Invalid
        | ErrorCode::Unavailable
        | ErrorCode::Busy
        | ErrorCode::Forbidden
        | ErrorCode::Failed => true,
        ErrorCode::Unknown => false,
    }
}

const fn area(command: &Command) -> Option<&'static str> {
    Some(match command {
        Command::Runtime(_) => "runtime",
        Command::Bus(_) => "bus",
        Command::Events(_) => "events",
        Command::Attention(_) => "attention",
        Command::Usage(_) => "usage",
        Command::Models(_) => "models",
        Command::Health(_) => "health",
        Command::Opencode(_) => "opencode",
        Command::Environments(_) => "environments",
        Command::Session(_) => "session",
        Command::Group(_) => "group",
        Command::Node(_) => "node",
        Command::Terminal(_) => "terminal",
        Command::Host(_) => "host",
        Command::Chat(_) => "chat",
        Command::Agent(_) => "agent",
        Command::Gateway(_) => "gateway",
        Command::Settings(_) => "settings",
        Command::Git(_) => "git",
        Command::Changes(_) => "changes",
        Command::Kraddie(_) => "kraddie",
        Command::Variant(_) => "variant",
        Command::Review(_) => "review",
        Command::CodeHost(_) => "code_host",
        Command::Runner(_) => "runner",
        Command::Cloud(_) => "cloud",
        Command::Repo(_) => "repo",
        Command::File(_) => "file",
        Command::Editor(_) => "editor",
        Command::Unknown => return None,
    })
}

fn canonical<T: DeserializeOwned + Serialize>(example: Value) -> serde_json::Result<Value> {
    serde_json::to_value(serde_json::from_value::<T>(example)?)
}

fn canonical_map<T: DeserializeOwned + Serialize>(
    examples: Vec<(&str, Value)>,
) -> Result<Map<String, Value>, String> {
    examples
        .into_iter()
        .map(|(name, example)| {
            canonical::<T>(example)
                .map(|value| (name.to_owned(), value))
                .map_err(|error| format!("{name}: {error}"))
        })
        .collect()
}

fn names<T: Serialize>(items: &[T]) -> serde_json::Result<Vec<Value>> {
    items.iter().map(serde_json::to_value).collect()
}

fn description() -> Result<Value, Box<dyn std::error::Error>> {
    let forward: Vec<Value> = examples::forward_compatible()
        .into_iter()
        .map(|(line, reads_as)| {
            Ok(json!({ "line": line, "reads_as": canonical::<Message>(reads_as)? }))
        })
        .collect::<serde_json::Result<_>>()?;
    let session: Vec<Value> = examples::session()
        .into_iter()
        .map(|(from, message)| {
            Ok(json!({ "from": from, "message": canonical::<Message>(message)? }))
        })
        .collect::<serde_json::Result<_>>()?;
    Ok(json!({
        "title": "Krad ADE runtime protocol",
        "protocol_version": ProtocolVersion::CURRENT,
        "versioning": examples::VERSIONING,
        "transport": examples::TRANSPORT,
        "frames": {
            "length_prefix_bytes": LENGTH_BYTES,
            "length": "big-endian unsigned length of the body that follows, kind byte included",
            "max_frame_bytes": MAX_FRAME_BYTES,
            "kinds": {
                "message": FrameKind::Message as u8,
                "terminal": FrameKind::Terminal as u8,
            },
            "message": "the kind byte, then one message as UTF-8 JSON",
            "terminal": "the kind byte, the terminal id as a big-endian u64, then the raw bytes",
            "terminal_id_bytes": TERMINAL_ID_BYTES,
        },
        "handshake": examples::HANDSHAKE,
        "resume": examples::RESUME,
        "idempotency": examples::IDEMPOTENCY,
        "paths": examples::PATHS,
        "error_codes": names(&ERROR_CODES)?,
        "scopes": names(&SCOPES)?,
        "command_areas": AREAS,
        "messages": canonical_map::<Message>(examples::messages())?,
        "commands": canonical_map::<Command>(examples::commands())?,
        "events": canonical_map::<Feed>(examples::events())?,
        "forward_compatible": forward,
        "example_session": session,
    }))
}

fn published_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(SCHEMA_FILE)
}

fn published() -> Result<Value, Box<dyn std::error::Error>> {
    Ok(serde_json::from_str(&std::fs::read_to_string(
        published_path(),
    )?)?)
}

fn exact<T: DeserializeOwned + Serialize>(example: &Value) -> TestResult {
    let written = serde_json::to_value(serde_json::from_value::<T>(example.clone())?)?;
    assert_eq!(
        &written, example,
        "the description must match the wire exactly"
    );
    Ok(())
}

fn entries(value: &Value) -> impl Iterator<Item = &Value> {
    value.as_object().into_iter().flat_map(|map| map.values())
}

#[test]
fn the_published_description_is_the_generated_one() -> TestResult {
    let generated = description()?;
    if std::env::var(WRITE_VAR).as_deref() == Ok(WRITE_VALUE) {
        let pretty = format!("{}\n", serde_json::to_string_pretty(&generated)?);
        std::fs::write(published_path(), pretty)?;
    }
    assert!(
        published().ok() == Some(generated),
        "the description is stale: run `{WRITE_VAR}={WRITE_VALUE} cargo test -p krad-protocol schema`"
    );
    Ok(())
}

#[test]
fn every_example_reads_and_writes_back_as_itself() -> TestResult {
    let api = published()?;
    assert_eq!(api["protocol_version"], ProtocolVersion::CURRENT.0);
    entries(&api["messages"]).try_for_each(exact::<Message>)?;
    entries(&api["commands"]).try_for_each(exact::<Command>)?;
    entries(&api["events"]).try_for_each(exact::<Feed>)?;
    let session = api["example_session"].as_array().ok_or("a session")?;
    assert!(session.len() > 2);
    for step in session {
        assert!(matches!(step["from"].as_str(), Some("client" | "runtime")));
        exact::<Message>(&step["message"])?;
    }
    Ok(())
}

#[test]
fn a_newer_peer_s_lines_read_as_the_listed_older_messages() -> TestResult {
    let api = published()?;
    let cases = api["forward_compatible"]
        .as_array()
        .ok_or("forward cases")?;
    let [_, ..] = cases.as_slice() else {
        return Err("no forward cases".into());
    };
    for case in cases {
        let read = serde_json::to_value(serde_json::from_value::<Message>(case["line"].clone())?)?;
        assert_eq!(read, case["reads_as"], "{}", case["line"]);
    }
    Ok(())
}

#[test]
fn the_listed_codes_scopes_and_areas_are_the_ones_the_protocol_speaks() -> TestResult {
    assert!(ERROR_CODES.into_iter().all(listed_code));
    let api = published()?;
    let mut spoken: Vec<&str> = entries(&api["commands"])
        .filter_map(|command| command["area"].as_str())
        .collect();
    spoken.sort_unstable();
    spoken.dedup();
    let mut areas = AREAS.to_vec();
    areas.sort_unstable();
    assert_eq!(spoken, areas, "every command area has an example");
    for command in entries(&api["commands"]) {
        let read: Command = serde_json::from_value(command.clone())?;
        assert_eq!(area(&read), command["area"].as_str());
    }
    for code in ERROR_CODES {
        let reply = Reply::Refused {
            code,
            message: String::new(),
        };
        let json = serde_json::to_value(&reply)?;
        assert!(api["error_codes"]
            .as_array()
            .is_some_and(|codes| codes.contains(&json["code"])));
    }
    Ok(())
}
