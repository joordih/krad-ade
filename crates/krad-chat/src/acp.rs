mod chat;
mod client_side;
mod config;
mod connection;
mod content;
mod files;
mod ids;
mod lenient;
mod managed;
mod methods;
mod package;
mod paths;
mod permission;
mod prompt;
mod rpc;
mod setup;
mod terminals;
mod update;

pub(crate) use chat::spawn_acp;
pub use chat::{acp_agent_env, AcpConfig, AcpWanted, MissingSecret, Variables};
pub use client_side::{ClientSide, FileWritten, HostTerminals, OpenBuffers, PermissionAsked};
pub use config::{
    ConfigKind, ConfigSelectEntry, ConfigSelectGroup, ConfigSelectOption, SessionConfigOption,
    SetSessionConfigOptionRequest, SetSessionConfigOptionResponse,
};
pub use connection::{AcpClient, AcpConnection, AcpError, AgentRequest, Pending, Responder};
pub use content::{
    AudioContent, BlobResourceContents, ContentBlock, EmbeddedResource, EmbeddedResourceResource,
    ImageContent, ResourceLink, TextContent, TextResourceContents,
};
pub use files::{ReadTextFileRequest, ReadTextFileResponse, WriteTextFileRequest};
pub use ids::{
    AuthMethodId, ConfigOptionId, ModeId, ModelId, PermissionOptionId, SessionId, TerminalId,
    ToolCallId,
};
pub use managed::{install_managed_adapter, managed_adapter_program, managed_adapter_update_due};
pub use package::{acp_package_state, install_acp_package, AcpPackageError};
pub use paths::AgentPaths;
pub use permission::{
    PermissionOption, PermissionOptionKind, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionResponse,
};
pub use prompt::{CancelNotification, PromptRequest, PromptResponse, StopReason};
pub use rpc::{EmptyResponse, ErrorCode, RequestId, RpcError};
pub use setup::{
    AgentAuthMethod, AgentCapabilities, AuthCapabilities, AuthMethod, AuthenticateRequest,
    ClientCapabilities, EnvVariable, FileSystemCapabilities, HttpHeader, Implementation,
    InitializeRequest, InitializeResponse, LoadSessionRequest, LoadSessionResponse,
    McpCapabilities, McpServer, ModelInfo, NewSessionRequest, NewSessionResponse,
    PromptCapabilities, ProtocolVersion, RemoteMcpServer, SessionMode, SessionModeState,
    SessionModelState, SetSessionModeRequest, SetSessionModelRequest, StdioMcpServer,
    TerminalAuthMethod,
};
pub use terminals::{
    CreateTerminalRequest, CreateTerminalResponse, TerminalExitStatus, TerminalOutputResponse,
    TerminalRequest,
};
pub use update::{
    AvailableCommand, AvailableCommandInput, AvailableCommandsUpdate, ConfigOptionUpdate,
    ContentChunk, CurrentModeUpdate, Diff, Plan, PlanEntry, PlanEntryPriority, PlanEntryStatus,
    SessionCost, SessionNotification, SessionUpdate, ToolCall, ToolCallContent, ToolCallLocation,
    ToolCallStatus, ToolCallUpdate, ToolKind, UsageUpdate,
};
