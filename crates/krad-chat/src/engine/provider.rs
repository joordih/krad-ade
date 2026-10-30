mod completions;
mod messages;
mod responses;
mod sse;

use krad_domain::{Effort, GatewayAccess};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use ureq::Agent;

use super::wire::{Block, Delta, Protocol, Reply, Request};
use crate::gateway::{GatewayError, AUTHORIZATION, BEARER};
pub(crate) use sse::Events;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const ERROR_BODY_LIMIT: u64 = 64 * 1024;
const DETAIL_CHARS: usize = 300;
const ACCEPT: &str = "Accept";
const EVENT_STREAM: &str = "text/event-stream";
pub(super) const IMAGE_DETAIL: &str = "auto";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ReasoningEffort {
    Low,
    Medium,
    High,
}

impl From<Effort> for ReasoningEffort {
    fn from(effort: Effort) -> Self {
        match effort {
            Effort::Low => Self::Low,
            Effort::Medium => Self::Medium,
            Effort::High | Effort::ExtraHigh | Effort::Max => Self::High,
        }
    }
}

pub(crate) trait Parse {
    fn feed(&mut self, data: &str, on_delta: &mut dyn FnMut(Delta)) -> Result<(), GatewayError>;

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

fn status_error(code: u16, body: &str) -> GatewayError {
    if GatewayError::rejects(code) && error_message(body).is_none() {
        return GatewayError::Rejected;
    }
    GatewayError::Status {
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

pub(crate) fn read_stream<P: Parse>(
    reader: impl BufRead,
    mut parser: P,
    cancel: &AtomicBool,
    on_delta: &mut dyn FnMut(Delta),
) -> Result<Reply, GatewayError> {
    for data in Events::new(reader) {
        if cancel.load(Ordering::SeqCst) {
            return Err(GatewayError::Cancelled);
        }
        let data = data.map_err(|error| GatewayError::Unreachable(error.to_string()))?;
        parser.feed(&data, on_delta)?;
    }
    if cancel.load(Ordering::SeqCst) {
        return Err(GatewayError::Cancelled);
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
) -> Result<Reply, GatewayError> {
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
) -> Result<Reply, GatewayError> {
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
