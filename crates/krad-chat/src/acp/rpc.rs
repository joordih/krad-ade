use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

const JSONRPC_VERSION: &str = "2.0";

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    Number(i64),
    Text(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ErrorCode(pub i32);

impl ErrorCode {
    pub const PARSE_ERROR: Self = Self(-32700);
    pub const INVALID_REQUEST: Self = Self(-32600);
    pub const METHOD_NOT_FOUND: Self = Self(-32601);
    pub const INVALID_PARAMS: Self = Self(-32602);
    pub const INTERNAL_ERROR: Self = Self(-32603);
    pub const REQUEST_CANCELLED: Self = Self(-32800);
    pub const AUTH_REQUIRED: Self = Self(-32000);
    pub const RESOURCE_NOT_FOUND: Self = Self(-32002);
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Error)]
#[error("{message}")]
pub struct RpcError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl RpcError {
    #[must_use]
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmptyResponse {}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum Incoming {
    Request {
        id: RequestId,
        method: String,
        #[serde(default)]
        params: Value,
    },
    Notification {
        method: String,
        #[serde(default)]
        params: Value,
    },
    Failure {
        id: RequestId,
        error: RpcError,
    },
    Success {
        id: RequestId,
        #[serde(default)]
        result: Value,
    },
}

#[derive(Serialize)]
pub(super) struct OutgoingRequest<'a, P> {
    jsonrpc: &'static str,
    id: u64,
    method: &'a str,
    params: &'a P,
}

impl<'a, P> OutgoingRequest<'a, P> {
    pub(super) const fn new(id: u64, method: &'a str, params: &'a P) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION,
            id,
            method,
            params,
        }
    }
}

#[derive(Serialize)]
pub(super) struct OutgoingNotification<'a, P> {
    jsonrpc: &'static str,
    method: &'a str,
    params: &'a P,
}

impl<'a, P> OutgoingNotification<'a, P> {
    pub(super) const fn new(method: &'a str, params: &'a P) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION,
            method,
            params,
        }
    }
}

#[derive(Serialize)]
pub(super) struct OutgoingResult<'a, R> {
    jsonrpc: &'static str,
    id: &'a RequestId,
    result: &'a R,
}

impl<'a, R> OutgoingResult<'a, R> {
    pub(super) const fn new(id: &'a RequestId, result: &'a R) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION,
            id,
            result,
        }
    }
}

#[derive(Serialize)]
pub(super) struct OutgoingError<'a> {
    jsonrpc: &'static str,
    id: &'a RequestId,
    error: &'a RpcError,
}

impl<'a> OutgoingError<'a> {
    pub(super) const fn new(id: &'a RequestId, error: &'a RpcError) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION,
            id,
            error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_kind_of_line_reads_as_what_it_is() {
        let lines = [
            r#"{"jsonrpc":"2.0","id":"a","method":"fs/read_text_file","params":{}}"#,
            r#"{"jsonrpc":"2.0","method":"session/update","params":{}}"#,
            r#"{"jsonrpc":"2.0","id":4,"error":{"code":-32000,"message":"Authentication required"}}"#,
            r#"{"jsonrpc":"2.0","id":5,"result":null}"#,
        ];
        let kinds: Vec<&str> = lines
            .iter()
            .map(|line| match serde_json::from_str::<Incoming>(line) {
                Ok(Incoming::Request { .. }) => "request",
                Ok(Incoming::Notification { .. }) => "notification",
                Ok(Incoming::Failure { error, .. }) if error.code == ErrorCode::AUTH_REQUIRED => {
                    "failure"
                }
                Ok(Incoming::Success { .. }) => "success",
                _ => "unreadable",
            })
            .collect();
        assert_eq!(kinds, ["request", "notification", "failure", "success"]);
    }
}
