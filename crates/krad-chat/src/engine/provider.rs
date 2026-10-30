mod completions;
mod messages;
mod responses;
mod sse;

use krad_domain::GatewayAccess;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use thiserror::Error;
use ureq::Agent;

use super::wire::{Block, Delta, Protocol, Reply, Request};
use sse::{Event, Events};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const ERROR_BODY_LIMIT: u64 = 64 * 1024;
const DETAIL_CHARS: usize = 300;
const AUTHORIZATION: &str = "Authorization";
const BEARER: &str = "Bearer";
const ACCEPT: &str = "Accept";
const EVENT_STREAM: &str = "text/event-stream";
const UNAUTHORIZED: u16 = 401;
const FORBIDDEN: u16 = 403;
const ANTHROPIC_OWNERS: [&str; 2] = ["anthropic", "claude"];
const OPENAI_OWNERS: [&str; 3] = ["openai", "codex", "azure-openai"];
const ANTHROPIC_PREFIXES: [&str; 1] = ["claude"];
const OPENAI_PREFIXES: [&str; 5] = ["gpt-", "o1", "o3", "o4", "codex"];

#[derive(Debug, Error)]
pub(crate) enum ProviderError {
    #[error("the Gateway rejected the key")]
    Rejected,
    #[error("the Gateway answered with status {code}: {detail}")]
    Status { code: u16, detail: String },
    #[error("the Gateway could not be reached: {0}")]
    Unreachable(String),
    #[error("the Gateway sent an answer that could not be read: {0}")]
    Unreadable(String),
    #[error("the model stopped with an error: {0}")]
    Failed(String),
    #[error("the answer was stopped")]
    Cancelled,
}

impl From<ureq::Error> for ProviderError {
    fn from(error: ureq::Error) -> Self {
        match error {
            ureq::Error::StatusCode(UNAUTHORIZED | FORBIDDEN) => Self::Rejected,
            ureq::Error::StatusCode(code) => Self::Status {
                code,
                detail: String::new(),
            },
            error => Self::Unreachable(error.to_string()),
        }
    }
}

impl From<serde_json::Error> for ProviderError {
    fn from(error: serde_json::Error) -> Self {
        Self::Unreadable(error.to_string())
    }
}

pub(crate) trait Parse {
    fn feed(&mut self, event: &Event, on_delta: &mut dyn FnMut(Delta))
        -> Result<(), ProviderError>;

    fn finish(self) -> Reply;
}

#[derive(Deserialize)]
struct ErrorBody {
    #[serde(default)]
    error: Option<ErrorField>,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ErrorField {
    Detailed { message: String },
    Plain(String),
}

pub(crate) fn error_message(text: &str) -> Option<String> {
    let body: ErrorBody = serde_json::from_str(text).ok()?;
    match body.error {
        Some(ErrorField::Detailed { message } | ErrorField::Plain(message)) => Some(message),
        None => body.message,
    }
}

fn detail_of(text: &str) -> String {
    let detail = error_message(text).unwrap_or_else(|| text.trim().to_owned());
    match detail.char_indices().nth(DETAIL_CHARS) {
        Some((end, _)) => detail[..end].to_owned(),
        None => detail,
    }
}

fn status_error(code: u16, body: &str) -> ProviderError {
    if matches!(code, UNAUTHORIZED | FORBIDDEN) && error_message(body).is_none() {
        return ProviderError::Rejected;
    }
    ProviderError::Status {
        code,
        detail: detail_of(body),
    }
}

pub(crate) fn push_text(blocks: &mut Vec<Block>, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(Block::Text { text: last }) = blocks.last_mut() {
        last.push_str(text);
        return;
    }
    blocks.push(Block::Text {
        text: text.to_owned(),
    });
}

pub(crate) fn object_arguments(json: &str) -> Value {
    match serde_json::from_str(json) {
        Ok(value @ Value::Object(_)) => value,
        _ => Value::Object(Map::new()),
    }
}

fn owned_by(owner: &str, known: &[&str]) -> bool {
    known.iter().any(|name| owner.eq_ignore_ascii_case(name))
}

fn named_like(model: &str, prefixes: &[&str]) -> bool {
    let model = model.to_ascii_lowercase();
    prefixes.iter().any(|prefix| model.starts_with(prefix))
}

pub(crate) fn protocol_for(owner: Option<&str>, model: &str) -> Protocol {
    let owner = owner.map(str::trim).filter(|owner| !owner.is_empty());
    let known = |owners: &[&str], prefixes: &[&str]| {
        owner.map_or_else(
            || named_like(model, prefixes),
            |owner| owned_by(owner, owners),
        )
    };
    if known(&ANTHROPIC_OWNERS, &ANTHROPIC_PREFIXES) {
        Protocol::Messages
    } else if known(&OPENAI_OWNERS, &OPENAI_PREFIXES) {
        Protocol::Responses
    } else {
        Protocol::Completions
    }
}

pub(crate) fn read_stream<P: Parse>(
    reader: impl BufRead,
    mut parser: P,
    cancel: &AtomicBool,
    on_delta: &mut dyn FnMut(Delta),
) -> Result<Reply, ProviderError> {
    for event in Events::new(reader) {
        if cancel.load(Ordering::SeqCst) {
            return Err(ProviderError::Cancelled);
        }
        let event = event.map_err(|error| ProviderError::Unreachable(error.to_string()))?;
        parser.feed(&event, on_delta)?;
    }
    if cancel.load(Ordering::SeqCst) {
        return Err(ProviderError::Cancelled);
    }
    Ok(parser.finish())
}

fn post<B: Serialize, P: Parse>(
    access: &GatewayAccess,
    path: &str,
    headers: &[(&str, &str)],
    body: &B,
    parser: P,
    cancel: &AtomicBool,
    on_delta: &mut dyn FnMut(Delta),
) -> Result<Reply, ProviderError> {
    let agent: Agent = Agent::config_builder()
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .http_status_as_error(false)
        .build()
        .into();
    let mut request = agent
        .post(access.url.join(path))
        .header(AUTHORIZATION, format!("{BEARER} {}", access.key.expose()))
        .header(ACCEPT, EVENT_STREAM);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let mut response = request.send_json(body)?;
    let code = response.status().as_u16();
    if !response.status().is_success() {
        let text = response
            .body_mut()
            .with_config()
            .limit(ERROR_BODY_LIMIT)
            .read_to_string()
            .unwrap_or_default();
        return Err(status_error(code, &text));
    }
    let reader = BufReader::new(response.into_body().into_reader());
    read_stream(reader, parser, cancel, on_delta)
}

pub(crate) fn stream(
    access: &GatewayAccess,
    protocol: Protocol,
    request: &Request<'_>,
    cancel: &AtomicBool,
    on_delta: &mut dyn FnMut(Delta),
) -> Result<Reply, ProviderError> {
    match protocol {
        Protocol::Messages => post(
            access,
            messages::PATH,
            &messages::HEADERS,
            &messages::body(request),
            messages::Parser::default(),
            cancel,
            on_delta,
        ),
        Protocol::Responses => post(
            access,
            responses::PATH,
            &[],
            &responses::body(request),
            responses::Parser::default(),
            cancel,
            on_delta,
        ),
        Protocol::Completions => post(
            access,
            completions::PATH,
            &[],
            &completions::body(request),
            completions::Parser::default(),
            cancel,
            on_delta,
        ),
    }
}

#[cfg(test)]
mod tests;
