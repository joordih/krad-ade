use krad_domain::{
    AcpWanted, AgentPreset, ChatMessage, Effort, LiveSession, McpServerName, ModelId,
    PermissionMode,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ChatOpen {
    pub harness: AgentPreset,
    pub model: Option<ModelId>,
    pub effort: Option<Effort>,
    pub permission_mode: Option<PermissionMode>,
    pub mcp_servers: Option<Vec<McpServerName>>,
    pub acp: AcpWanted,
    pub session: LiveSession,
    pub messages: Vec<ChatMessage>,
    pub notices: Vec<String>,
}
