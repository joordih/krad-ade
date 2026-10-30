use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::McpError;

pub(super) const PROTOCOL_VERSION: &str = "2025-06-18";
pub(super) const INITIALIZE: &str = "initialize";
pub(super) const INITIALIZED: &str = "notifications/initialized";
pub(super) const TOOLS_LIST: &str = "tools/list";
pub(super) const TOOLS_CALL: &str = "tools/call";
pub(super) const PING: &str = "ping";
const NAME_FIELD: &str = "name";
const ARGUMENTS_FIELD: &str = "arguments";
const JSONRPC_VERSION: &str = "2.0";
const CLIENT_NAME: &str = "krad-ade";
const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const METHOD_NOT_FOUND: i64 = -32601;
const METHOD_NOT_FOUND_TEXT: &str = "Method not found";
const TEXT_JOINER: &str = "\n";
const TEXT_KIND: &str = "text";
const NO_CONTENT_TEXT: &str = "(no content)";
const NOT_SHOWN_OPEN: &str = "[";
const NOT_SHOWN_CLOSE: &str = " content not shown]";

#[derive(Serialize)]
struct Request<'a> {
    jsonrpc: &'static str,
    id: u64,
    method: &'a str,
    params: &'a Value,
}

#[derive(Serialize)]
struct Notification<'a> {
    jsonrpc: &'static str,
    method: &'a str,
}

#[derive(Serialize)]
struct Answer<'a> {
    jsonrpc: &'static str,
    id: &'a Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<AnswerError>,
}

#[derive(Serialize)]
struct AnswerError {
    code: i64,
    message: &'static str,
}

#[derive(Deserialize)]
pub(super) struct Incoming {
    #[serde(default)]
    pub(super) id: Option<Value>,
    #[serde(default)]
    pub(super) method: Option<String>,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<RpcError>,
}

#[derive(Deserialize)]
struct RpcError {
    code: i64,
    #[serde(default)]
    message: String,
}

pub(super) enum Received {
    Reply(Result<Value, McpError>),
    Request { id: Value, method: String },
    Other,
}

impl Incoming {
    pub(super) fn sort(self, waiting: u64) -> Received {
        match (self.method, self.id) {
            (Some(method), Some(id)) => Received::Request { id, method },
            (None, Some(id)) if id.as_u64() == Some(waiting) => {
                Received::Reply(match (self.result, self.error) {
                    (_, Some(error)) => Err(McpError::Server {
                        code: error.code,
                        message: error.message,
                    }),
                    (Some(result), None) => Ok(result),
                    (None, None) => Err(McpError::Malformed),
                })
            }
            _ => Received::Other,
        }
    }
}

pub(super) fn request_line(id: u64, method: &str, params: &Value) -> String {
    serde_json::to_string(&Request {
        jsonrpc: JSONRPC_VERSION,
        id,
        method,
        params,
    })
    .unwrap_or_default()
}

pub(super) fn notification_line(method: &str) -> String {
    serde_json::to_string(&Notification {
        jsonrpc: JSONRPC_VERSION,
        method,
    })
    .unwrap_or_default()
}

pub(super) fn answer_line(id: &Value, method: &str) -> String {
    let answer = if method == PING {
        Answer {
            jsonrpc: JSONRPC_VERSION,
            id,
            result: Some(json!({})),
            error: None,
        }
    } else {
        Answer {
            jsonrpc: JSONRPC_VERSION,
            id,
            result: None,
            error: Some(AnswerError {
                code: METHOD_NOT_FOUND,
                message: METHOD_NOT_FOUND_TEXT,
            }),
        }
    };
    serde_json::to_string(&answer).unwrap_or_default()
}

pub(super) fn initialize_params() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": {},
        "clientInfo": { "name": CLIENT_NAME, "version": CLIENT_VERSION },
    })
}

pub(super) fn list_params(cursor: Option<&str>) -> Value {
    cursor.map_or_else(|| json!({}), |cursor| json!({ "cursor": cursor }))
}

pub(super) fn call_params(tool: &str, arguments: Value) -> Value {
    let mut params = Map::new();
    params.insert(NAME_FIELD.to_owned(), Value::from(tool));
    params.insert(ARGUMENTS_FIELD.to_owned(), arguments);
    Value::Object(params)
}

#[derive(Deserialize)]
pub(super) struct ListedTool {
    pub(super) name: String,
    #[serde(default)]
    pub(super) description: Option<String>,
    #[serde(default, rename = "inputSchema")]
    pub(super) input_schema: Option<Value>,
}

#[derive(Deserialize)]
pub(super) struct ToolPage {
    #[serde(default)]
    pub(super) tools: Vec<ListedTool>,
    #[serde(default, rename = "nextCursor")]
    pub(super) next_cursor: Option<String>,
}

#[derive(Deserialize)]
struct Content {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Deserialize)]
struct CallResult {
    #[serde(default)]
    content: Vec<Content>,
    #[serde(default, rename = "isError")]
    is_error: bool,
}

pub(super) fn call_text(result: Value) -> Result<String, McpError> {
    let result: CallResult = serde_json::from_value(result).map_err(|_| McpError::Malformed)?;
    let pieces: Vec<String> = result
        .content
        .into_iter()
        .map(|content| match (content.kind.as_str(), content.text) {
            (TEXT_KIND, Some(text)) => text,
            (kind, _) => format!("{NOT_SHOWN_OPEN}{kind}{NOT_SHOWN_CLOSE}"),
        })
        .collect();
    let text = if pieces.is_empty() {
        NO_CONTENT_TEXT.to_owned()
    } else {
        pieces.join(TEXT_JOINER)
    };
    if result.is_error {
        return Err(McpError::ToolFailed(text));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn incoming(text: &str) -> Incoming {
        serde_json::from_str(text).unwrap_or_else(|error| unreachable!("{error}"))
    }

    #[test]
    fn replies_are_matched_by_id_and_requests_are_told_apart() {
        assert!(matches!(
            incoming(r#"{"jsonrpc":"2.0","id":3,"result":{"ok":true}}"#).sort(3),
            Received::Reply(Ok(_))
        ));
        assert!(matches!(
            incoming(r#"{"jsonrpc":"2.0","id":2,"result":{}}"#).sort(3),
            Received::Other
        ));
        assert!(matches!(
            incoming(r#"{"jsonrpc":"2.0","id":3,"error":{"code":-32602,"message":"bad"}}"#).sort(3),
            Received::Reply(Err(McpError::Server { code: -32602, .. }))
        ));
        assert!(matches!(
            incoming(r#"{"jsonrpc":"2.0","id":"s1","method":"ping"}"#).sort(3),
            Received::Request { .. }
        ));
        assert!(matches!(
            incoming(r#"{"jsonrpc":"2.0","method":"notifications/message"}"#).sort(3),
            Received::Other
        ));
    }

    #[test]
    fn pings_are_answered_and_other_requests_refused() {
        let id = json!("s1");
        assert!(answer_line(&id, PING).contains(r#""result":{}"#));
        assert!(answer_line(&id, "roots/list").contains("-32601"));
    }

    #[test]
    fn text_items_are_joined_and_others_noted() {
        let text = call_text(json!({"content":[
            {"type":"text","text":"one"},
            {"type":"image","data":"…","mimeType":"image/png"},
            {"type":"text","text":"two"}
        ]}));
        assert_eq!(
            text.ok().as_deref(),
            Some("one\n[image content not shown]\ntwo")
        );
        assert!(matches!(
            call_text(json!({"content":[{"type":"text","text":"boom"}],"isError":true})),
            Err(McpError::ToolFailed(text)) if text == "boom"
        ));
        assert_eq!(
            call_text(json!({"content":[]})).ok().as_deref(),
            Some(NO_CONTENT_TEXT)
        );
    }
}
