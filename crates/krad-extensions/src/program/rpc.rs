use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufRead, Write};

use crate::program::error::ProgramError;

const CONTENT_LENGTH: &str = "Content-Length:";
const HEADER_END: &str = "\r\n";
const JSONRPC_VERSION: &str = "2.0";
use krad_extension_api::limits::{MAX_FRAME_BYTES, MAX_HEADER_LINE_BYTES};
const NEWLINE: u8 = b'\n';

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseError {
    pub code: i64,
    pub message: String,
}

pub(crate) const METHOD_NOT_FOUND: i64 = -32601;
pub(crate) const INVALID_PARAMS: i64 = -32602;
pub(crate) const REQUEST_FAILED: i64 = -32803;

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
pub(crate) enum Incoming {
    Response {
        id: i64,
        outcome: Result<Value, ResponseError>,
    },
    Notification {
        method: String,
        params: Value,
    },
    Request {
        id: Value,
        method: String,
        params: Value,
    },
}

impl Incoming {
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
                Some(Self::Response {
                    id: id.as_i64()?,
                    outcome,
                })
            }
            (None, None) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Outgoing {
    Request {
        id: i64,
        method: &'static str,
        params: Value,
    },
    Notification {
        method: &'static str,
        params: Value,
    },
    Reply {
        id: Value,
        outcome: Result<Value, ResponseError>,
    },
}

#[derive(Serialize)]
struct Frame<'a> {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    method: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<&'a Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<&'a Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a ResponseError>,
}

impl<'a> From<&'a Outgoing> for Frame<'a> {
    fn from(message: &'a Outgoing) -> Self {
        let blank = Self {
            jsonrpc: JSONRPC_VERSION,
            id: None,
            method: None,
            params: None,
            result: None,
            error: None,
        };
        match message {
            Outgoing::Request { id, method, params } => Self {
                id: Some(Value::from(*id)),
                method: Some(method),
                params: Some(params),
                ..blank
            },
            Outgoing::Notification { method, params } => Self {
                method: Some(method),
                params: Some(params),
                ..blank
            },
            Outgoing::Reply { id, outcome } => Self {
                id: Some(id.clone()),
                result: outcome.as_ref().ok(),
                error: outcome.as_ref().err(),
                ..blank
            },
        }
    }
}

pub(crate) fn write_message(out: &mut impl Write, message: &Outgoing) -> Result<(), ProgramError> {
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

pub(crate) struct BoundedLine {
    pub(crate) bytes: Vec<u8>,
    pub(crate) truncated: bool,
}

pub(crate) fn read_bounded_line(
    input: &mut impl BufRead,
    max: usize,
) -> std::io::Result<Option<BoundedLine>> {
    let mut line = BoundedLine {
        bytes: Vec::new(),
        truncated: false,
    };
    let mut read_any = false;
    loop {
        let available = input.fill_buf()?;
        if available.is_empty() {
            return Ok(read_any.then_some(line));
        }
        read_any = true;
        let (chunk, done) = match available.iter().position(|byte| *byte == NEWLINE) {
            Some(at) => (&available[..=at], true),
            None => (available, false),
        };
        let room = max.saturating_sub(line.bytes.len());
        if chunk.len() > room {
            line.truncated = true;
        }
        line.bytes
            .extend_from_slice(&chunk[..chunk.len().min(room)]);
        let used = chunk.len();
        input.consume(used);
        if done {
            return Ok(Some(line));
        }
    }
}

pub(crate) fn read_message(input: &mut impl BufRead) -> Result<Option<Incoming>, ProgramError> {
    let mut length = None;
    loop {
        let Some(read) = read_bounded_line(input, MAX_HEADER_LINE_BYTES)? else {
            return Ok(None);
        };
        if read.truncated {
            return Err(ProgramError::Header);
        }
        let text = String::from_utf8_lossy(&read.bytes);
        let line = text.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix(CONTENT_LENGTH) {
            length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| ProgramError::Header)?,
            );
        }
    }
    let length = length.ok_or(ProgramError::Header)?;
    if length > MAX_FRAME_BYTES {
        return Err(ProgramError::FrameTooLarge(length));
    }
    let mut body = vec![0; length];
    input.read_exact(&mut body)?;
    let raw: RawMessage = serde_json::from_slice(&body)?;
    Ok(Incoming::from_raw(raw))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Cursor;

    fn framed(bodies: &[&str]) -> Cursor<Vec<u8>> {
        let stream = bodies
            .iter()
            .map(|body| format!("Content-Length: {}\r\n\r\n{body}", body.len()))
            .collect::<Vec<_>>()
            .concat();
        Cursor::new(stream.into_bytes())
    }

    #[test]
    fn a_written_request_frames_its_body_with_its_length() -> Result<(), ProgramError> {
        let mut out = Vec::new();
        let message = Outgoing::Request {
            id: 1,
            method: "shutdown",
            params: json!({}),
        };
        write_message(&mut out, &message)?;
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"shutdown","params":{}}"#;
        assert_eq!(
            String::from_utf8_lossy(&out),
            format!("Content-Length: {}\r\n\r\n{body}", body.len())
        );
        Ok(())
    }

    #[test]
    fn a_refused_reply_carries_its_error_and_no_result() -> Result<(), ProgramError> {
        let mut out = Vec::new();
        let message = Outgoing::Reply {
            id: json!("a"),
            outcome: Err(ResponseError {
                code: METHOD_NOT_FOUND,
                message: "no".to_owned(),
            }),
        };
        write_message(&mut out, &message)?;
        let text = String::from_utf8_lossy(&out);
        assert!(
            text.ends_with(r#"{"jsonrpc":"2.0","id":"a","error":{"code":-32601,"message":"no"}}"#)
        );
        Ok(())
    }

    #[test]
    fn responses_notifications_and_requests_read_apart() -> Result<(), ProgramError> {
        let mut input = framed(&[
            r#"{"jsonrpc":"2.0","id":7,"result":{"ok":true}}"#,
            r#"{"jsonrpc":"2.0","method":"window/toast","params":{"message":"hi"}}"#,
            r#"{"jsonrpc":"2.0","id":"a","method":"window/pick","params":{}}"#,
        ]);
        let mut read = Vec::new();
        while let Some(message) = read_message(&mut input)? {
            read.push(message);
        }
        assert_eq!(
            read[0],
            Incoming::Response {
                id: 7,
                outcome: Ok(json!({"ok": true}))
            }
        );
        assert!(
            matches!(&read[1], Incoming::Notification { method, .. } if method == "window/toast")
        );
        assert!(matches!(&read[2], Incoming::Request { id, .. } if *id == json!("a")));
        Ok(())
    }

    #[test]
    fn an_endless_header_line_is_refused_without_reading_it_all() {
        let endless = "X".repeat(MAX_HEADER_LINE_BYTES * 4);
        let mut input = Cursor::new(endless.into_bytes());
        assert!(matches!(
            read_message(&mut input),
            Err(ProgramError::Header)
        ));
    }

    #[test]
    fn a_bounded_line_keeps_its_head_and_says_it_was_cut() -> std::io::Result<()> {
        let mut input = Cursor::new(b"abcdefgh\nnext\n".to_vec());
        let first = read_bounded_line(&mut input, 3)?.ok_or(std::io::ErrorKind::UnexpectedEof)?;
        assert_eq!(first.bytes, b"abc");
        assert!(first.truncated);
        let second = read_bounded_line(&mut input, 16)?.ok_or(std::io::ErrorKind::UnexpectedEof)?;
        assert_eq!(second.bytes, b"next\n");
        assert!(!second.truncated);
        assert!(read_bounded_line(&mut input, 16)?.is_none());
        Ok(())
    }

    #[test]
    fn frames_without_a_length_or_too_large_are_refused() {
        let mut missing = Cursor::new(b"X-Other: 1\r\n\r\n{}".to_vec());
        assert!(matches!(
            read_message(&mut missing),
            Err(ProgramError::Header)
        ));
        let huge = format!("Content-Length: {}\r\n\r\n", MAX_FRAME_BYTES + 1);
        let mut huge = Cursor::new(huge.into_bytes());
        assert!(matches!(
            read_message(&mut huge),
            Err(ProgramError::FrameTooLarge(_))
        ));
    }
}
