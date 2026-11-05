mod chat;
mod config;
mod connection;
mod content;
mod files;
mod ids;
mod lenient;
mod methods;
mod permission;
mod prompt;
mod rpc;
mod setup;
mod terminals;
mod update;

pub(crate) use chat::spawn_acp;
pub use chat::AcpConfig;
pub use config::{
    ConfigCategory, ConfigKind, ConfigSelectEntry, ConfigSelectGroup, ConfigSelectOption,
    SessionConfigOption, SetSessionConfigOptionRequest, SetSessionConfigOptionResponse,
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
    SessionNotification, SessionUpdate, ToolCall, ToolCallContent, ToolCallLocation,
    ToolCallStatus, ToolCallUpdate, ToolKind, UsageUpdate,
};
