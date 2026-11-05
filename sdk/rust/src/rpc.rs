use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufRead, Write};

use crate::error::SdkError;

const CONTENT_LENGTH: &str = "Content-Length:";
const HEADER_END: &str = "\r\n";
const JSONRPC_VERSION: &str = "2.0";
pub const MAX_FRAME_BYTES: usize = 64 * 1024 * 1024;

pub(crate) const METHOD_NOT_FOUND: i64 = -32601;
pub(crate) const INVALID_PARAMS: i64 = -32602;
pub(crate) const REQUEST_FAILED: i64 = -32803;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{message}")]
pub struct ResponseError {
    pub code: i64,
    pub message: String,
}

#[derive(Debug, Deserialize)]
struct RawMessage {
    #[serde(default)]
    id: Option<Value>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    params: Option<Value>,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<ResponseError>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    Notification {
        method: String,
        params: Value,
    },
    Response {
        id: Value,
        outcome: Result<Value, ResponseError>,
    },
}

impl Message {
    fn from_raw(raw: RawMessage) -> Option<Self> {
        let params = raw.params.unwrap_or(Value::Null);
        match (raw.id, raw.method) {
            (Some(id), Some(method)) => Some(Self::Request { id, method, params }),
            (None, Some(method)) => Some(Self::Notification { method, params }),
            (Some(id), None) => {
                let outcome = match raw.error {
                    Some(error) => Err(error),
                    None => Ok(raw.result.unwrap_or(Value::Null)),
                };
                Some(Self::Response { id, outcome })
            }
            (None, None) => None,
        }
    }
}

#[derive(Serialize)]
struct Frame<'a> {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<&'a Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    method: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<&'a Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<&'a Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a ResponseError>,
}

impl<'a> From<&'a Message> for Frame<'a> {
    fn from(message: &'a Message) -> Self {
        let blank = Self {
            jsonrpc: JSONRPC_VERSION,
            id: None,
            method: None,
            params: None,
            result: None,
            error: None,
        };
        match message {
            Message::Request { id, method, params } => Self {
                id: Some(id),
                method: Some(method),
                params: Some(params),
                ..blank
            },
            Message::Notification { method, params } => Self {
                method: Some(method),
                params: Some(params),
                ..blank
            },
            Message::Response { id, outcome } => Self {
                id: Some(id),
                result: outcome.as_ref().ok(),
                error: outcome.as_ref().err(),
                ..blank
            },
        }
    }
}

/// # Errors
///
/// Returns [`SdkError`] when the message cannot be encoded or written.
pub fn write_message(out: &mut impl Write, message: &Message) -> Result<(), SdkError> {
    let body = serde_json::to_vec(&Frame::from(message))?;
    write!(
        out,
        "{CONTENT_LENGTH} {}{HEADER_END}{HEADER_END}",
        body.len()
    )?;
    out.write_all(&body)?;
    out.flush()?;
    Ok(())
}

/// # Errors
///
/// Returns [`SdkError`] when the frame has no length, is too large, or is not JSON-RPC.
pub fn read_message(input: &mut impl BufRead) -> Result<Option<Message>, SdkError> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix(CONTENT_LENGTH) {
            length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| SdkError::Header)?,
            );
        }
    }
    let length = length.ok_or(SdkError::Header)?;
    if length > MAX_FRAME_BYTES {
        return Err(SdkError::FrameTooLarge(length));
    }
    let mut body = vec![0; length];
    input.read_exact(&mut body)?;
    let raw: RawMessage = serde_json::from_slice(&body)?;
    Ok(Message::from_raw(raw))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Cursor;

    #[test]
    fn every_kind_of_message_survives_a_round_trip() -> Result<(), SdkError> {
        let messages = [
            Message::Request {
                id: json!(1),
                method: "window/toast".to_owned(),
                params: json!({"message": "hi"}),
            },
            Message::Notification {
                method: "view/patch".to_owned(),
                params: json!({"view": "hello", "patches": []}),
            },
            Message::Response {
                id: json!("a"),
                outcome: Ok(json!({"features": []})),
            },
            Message::Response {
                id: json!(2),
                outcome: Err(ResponseError {
                    code: METHOD_NOT_FOUND,
                    message: "no".to_owned(),
                }),
            },
        ];
        let mut stream = Vec::new();
        for message in &messages {
            write_message(&mut stream, message)?;
        }
        let mut input = Cursor::new(stream);
        let mut read = Vec::new();
        while let Some(message) = read_message(&mut input)? {
            read.push(message);
        }
        assert_eq!(read, messages);
        Ok(())
    }

    #[test]
    fn frames_without_a_length_or_too_large_are_refused() {
        let mut missing = Cursor::new(b"X-Other: 1\r\n\r\n{}".to_vec());
        assert!(matches!(read_message(&mut missing), Err(SdkError::Header)));
        let huge = format!("Content-Length: {}\r\n\r\n", MAX_FRAME_BYTES + 1);
        assert!(matches!(
            read_message(&mut Cursor::new(huge.into_bytes())),
            Err(SdkError::FrameTooLarge(_))
        ));
    }
}
