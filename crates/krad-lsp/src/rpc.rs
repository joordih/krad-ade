use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufRead, Write};

use crate::error::LspError;

const CONTENT_LENGTH: &str = "Content-Length:";
const HEADER_END: &str = "\r\n";
const JSONRPC_VERSION: &str = "2.0";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct ResponseError {
    pub(crate) code: i64,
    pub(crate) message: String,
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

#[derive(Debug, PartialEq)]
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

#[derive(Serialize)]
struct Outgoing<'a> {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<&'a Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    method: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<&'a Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<&'a Value>,
}

pub(crate) enum Message<'a> {
    Request {
        id: &'a Value,
        method: &'a str,
        params: &'a Value,
    },
    Notification {
        method: &'a str,
        params: &'a Value,
    },
    Reply {
        id: &'a Value,
        result: &'a Value,
    },
}

impl<'a> From<Message<'a>> for Outgoing<'a> {
    fn from(message: Message<'a>) -> Self {
        let blank = Self {
            jsonrpc: JSONRPC_VERSION,
            id: None,
            method: None,
            params: None,
            result: None,
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
            Message::Reply { id, result } => Self {
                id: Some(id),
                result: Some(result),
                ..blank
            },
        }
    }
}

pub(crate) fn write_message(out: &mut impl Write, message: Message<'_>) -> Result<(), LspError> {
    let body = serde_json::to_vec(&Outgoing::from(message))?;
    write!(
        out,
        "{CONTENT_LENGTH} {}{HEADER_END}{HEADER_END}",
        body.len()
    )?;
    out.write_all(&body)?;
    out.flush()?;
    Ok(())
}

pub(crate) fn read_message(input: &mut impl BufRead) -> Result<Option<Incoming>, LspError> {
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
                    .map_err(|_| LspError::Header)?,
            );
        }
    }
    let mut body = vec![0; length.ok_or(LspError::Header)?];
    input.read_exact(&mut body)?;
    let raw: RawMessage = serde_json::from_slice(&body)?;
    Ok(Incoming::from_raw(raw))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Cursor;

    #[test]
    fn a_written_request_frames_its_body_with_its_length() -> Result<(), LspError> {
        let mut out = Vec::new();
        let (id, params) = (json!(1), json!({}));
        write_message(
            &mut out,
            Message::Request {
                id: &id,
                method: "shutdown",
                params: &params,
            },
        )?;
        let text = String::from_utf8_lossy(&out);
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"shutdown","params":{}}"#;
        assert_eq!(
            text,
            format!("Content-Length: {}\r\n\r\n{body}", body.len())
        );
        Ok(())
    }

    #[test]
    fn responses_notifications_and_requests_read_apart() -> Result<(), LspError> {
        let frames = [
            r#"{"jsonrpc":"2.0","id":7,"result":{"ok":true}}"#,
            r#"{"jsonrpc":"2.0","id":8,"error":{"code":-32601,"message":"no"}}"#,
            r#"{"jsonrpc":"2.0","method":"window/logMessage","params":{"message":"hi"}}"#,
            r#"{"jsonrpc":"2.0","id":"a","method":"workspace/configuration","params":{}}"#,
        ];
        let stream = frames
            .iter()
            .map(|body| format!("Content-Length: {}\r\n\r\n{body}", body.len()))
            .collect::<Vec<_>>()
            .concat();
        let mut input = Cursor::new(stream.into_bytes());
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
            matches!(&read[1], Incoming::Response { id: 8, outcome: Err(error) } if error.code == -32601)
        );
        assert!(
            matches!(&read[2], Incoming::Notification { method, .. } if method == "window/logMessage")
        );
        assert!(matches!(&read[3], Incoming::Request { id, .. } if *id == json!("a")));
        Ok(())
    }

    #[test]
    fn a_frame_without_its_length_is_refused() {
        let mut input = Cursor::new(b"X-Other: 1\r\n\r\n{}".to_vec());
        assert!(matches!(read_message(&mut input), Err(LspError::Header)));
    }
}
