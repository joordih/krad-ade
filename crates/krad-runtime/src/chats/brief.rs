use krad_chat::AcpWanted;
use krad_domain::{Effort, McpServerName, ModelId, PermissionMode};
use std::fmt;

use super::backend::{ChatLaunch, Choices};
use super::ChatPlace;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChatRole {
    #[default]
    Agent,
    Digest,
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct ChatBrief {
    pub model: Option<ModelId>,
    pub effort: Option<Effort>,
    pub permission_mode: Option<PermissionMode>,
    pub mcp_servers: Option<Vec<McpServerName>>,
    pub role: ChatRole,
    pub acp: AcpWanted,
}

impl fmt::Debug for ChatBrief {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ChatBrief")
            .field("model", &self.model)
            .field("effort", &self.effort)
            .field("permission_mode", &self.permission_mode)
            .field("role", &self.role)
            .field("acp", &self.acp)
            .finish_non_exhaustive()
    }
}

impl From<&ChatBrief> for Choices {
    fn from(brief: &ChatBrief) -> Self {
        Self {
            model: brief.model.clone(),
            effort: brief.effort,
            permission_mode: brief.permission_mode,
        }
    }
}

#[derive(Clone)]
pub struct PreparedLaunch {
    pub launch: ChatLaunch,
    pub place: ChatPlace,
}
