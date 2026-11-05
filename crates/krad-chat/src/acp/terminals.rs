use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::ids::{SessionId, TerminalId};
use super::setup::EnvVariable;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTerminalRequest {
    pub session_id: SessionId,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: Vec<EnvVariable>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_byte_limit: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTerminalResponse {
    pub terminal_id: TerminalId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalRequest {
    pub session_id: SessionId,
    pub terminal_id: TerminalId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOutputResponse {
    pub output: String,
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_status: Option<TerminalExitStatus>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalExitStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal: Option<String>,
}
