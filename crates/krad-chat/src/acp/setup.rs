use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use super::config::SessionConfigOption;
use super::ids::{AuthMethodId, ModeId, ModelId, SessionId};
use super::lenient::readable_items;

const REDACTED: &str = "[REDACTED]";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProtocolVersion(pub u16);

impl ProtocolVersion {
    pub const V1: Self = Self(1);
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Implementation {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub version: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FileSystemCapabilities {
    pub read_text_file: bool,
    pub write_text_file: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthCapabilities {
    pub terminal: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClientCapabilities {
    pub fs: FileSystemCapabilities,
    pub terminal: bool,
    pub auth: AuthCapabilities,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeRequest {
    pub protocol_version: ProtocolVersion,
    pub client_capabilities: ClientCapabilities,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_info: Option<Implementation>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PromptCapabilities {
    pub image: bool,
    pub audio: bool,
    pub embedded_context: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct McpCapabilities {
    pub http: bool,
    pub sse: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AgentCapabilities {
    pub load_session: bool,
    pub prompt_capabilities: PromptCapabilities,
    pub mcp_capabilities: McpCapabilities,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResponse {
    pub protocol_version: ProtocolVersion,
    #[serde(default)]
    pub agent_capabilities: AgentCapabilities,
    #[serde(default, deserialize_with = "readable_items")]
    pub auth_methods: Vec<AuthMethod>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_info: Option<Implementation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthMethod {
    Terminal(TerminalAuthMethod),
    #[serde(untagged)]
    Agent(AgentAuthMethod),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentAuthMethod {
    pub id: AuthMethodId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalAuthMethod {
    pub id: AuthMethodId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticateRequest {
    pub method_id: AuthMethodId,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvVariable {
    pub name: String,
    pub value: String,
}

impl fmt::Debug for EnvVariable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EnvVariable")
            .field("name", &self.name)
            .field("value", &REDACTED)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpHeader {
    pub name: String,
    pub value: String,
}

impl fmt::Debug for HttpHeader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HttpHeader")
            .field("name", &self.name)
            .field("value", &REDACTED)
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpServer {
    Http(RemoteMcpServer),
    Sse(RemoteMcpServer),
    #[serde(untagged)]
    Stdio(StdioMcpServer),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteMcpServer {
    pub name: String,
    pub url: String,
    pub headers: Vec<HttpHeader>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StdioMcpServer {
    pub name: String,
    pub command: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<EnvVariable>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewSessionRequest {
    pub cwd: PathBuf,
    pub mcp_servers: Vec<McpServer>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewSessionResponse {
    pub session_id: SessionId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modes: Option<SessionModeState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub models: Option<SessionModelState>,
    #[serde(
        default,
        deserialize_with = "readable_items",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub config_options: Vec<SessionConfigOption>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadSessionRequest {
    pub session_id: SessionId,
    pub cwd: PathBuf,
    pub mcp_servers: Vec<McpServer>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadSessionResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modes: Option<SessionModeState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub models: Option<SessionModelState>,
    #[serde(
        default,
        deserialize_with = "readable_items",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub config_options: Vec<SessionConfigOption>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionModeState {
    pub current_mode_id: ModeId,
    #[serde(default, deserialize_with = "readable_items")]
    pub available_modes: Vec<SessionMode>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionMode {
    pub id: ModeId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionModelState {
    pub current_model_id: ModelId,
    #[serde(default, deserialize_with = "readable_items")]
    pub available_models: Vec<ModelInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub model_id: ModelId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetSessionModeRequest {
    pub session_id: SessionId,
    pub mode_id: ModeId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetSessionModelRequest {
    pub session_id: SessionId,
    pub model_id: ModelId,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn stdio_servers_go_untagged_and_remote_ones_carry_their_transport() {
        let servers = vec![
            McpServer::Stdio(StdioMcpServer {
                name: "filesystem".to_owned(),
                command: PathBuf::from("/path/to/mcp-server"),
                args: vec!["--stdio".to_owned()],
                env: vec![EnvVariable {
                    name: "API_KEY".to_owned(),
                    value: "secret123".to_owned(),
                }],
            }),
            McpServer::Http(RemoteMcpServer {
                name: "api-server".to_owned(),
                url: "https://api.example.com/mcp".to_owned(),
                headers: Vec::new(),
            }),
        ];
        assert_eq!(
            serde_json::to_value(&servers).ok(),
            Some(json!([
                {"name": "filesystem", "command": "/path/to/mcp-server", "args": ["--stdio"], "env": [{"name": "API_KEY", "value": "secret123"}]},
                {"type": "http", "name": "api-server", "url": "https://api.example.com/mcp", "headers": []}
            ]))
        );
    }

    #[test]
    fn a_secret_value_never_shows_in_debug_output() {
        let variable = EnvVariable {
            name: "API_KEY".to_owned(),
            value: "secret123".to_owned(),
        };
        assert!(!format!("{variable:?}").contains("secret123"));
    }

    #[test]
    fn auth_methods_without_a_type_are_handled_by_the_agent_and_broken_ones_are_skipped() {
        let response: Result<InitializeResponse, _> = serde_json::from_value(json!({
            "protocolVersion": 1,
            "authMethods": [
                {"id": "oauth", "name": "Sign in"},
                {"type": "terminal", "id": "cli", "name": "Log in in a terminal", "args": ["login"]},
                {"name": "no id"}
            ]
        }));
        let methods = response.map_or_default(|response| response.auth_methods);
        assert!(matches!(
            methods.as_slice(),
            [AuthMethod::Agent(_), AuthMethod::Terminal(TerminalAuthMethod { args, .. })] if args == &["login"]
        ));
    }

    #[test]
    fn modes_and_models_of_a_new_session_are_read() {
        let response: Result<NewSessionResponse, _> = serde_json::from_value(json!({
            "sessionId": "sess_abc123def456",
            "modes": {
                "currentModeId": "ask",
                "availableModes": [
                    {"id": "ask", "name": "Ask", "description": "Request permission before making any changes"},
                    {"id": "code", "name": "Code"}
                ]
            },
            "models": {
                "currentModelId": "sonnet",
                "availableModels": [{"modelId": "sonnet", "name": "Sonnet"}]
            },
            "configOptions": []
        }));
        let response = response.ok();
        assert_eq!(
            response
                .as_ref()
                .and_then(|response| response.modes.as_ref())
                .map(|modes| modes.available_modes.len()),
            Some(2)
        );
        assert_eq!(
            response
                .and_then(|response| response.models)
                .map(|models| models.current_model_id),
            Some(ModelId("sonnet".to_owned()))
        );
    }
}
