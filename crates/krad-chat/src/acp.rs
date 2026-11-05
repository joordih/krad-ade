mod client_side;
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

pub use client_side::{ClientSide, FileWritten, HostTerminals, OpenBuffers};
pub use connection::{AcpClient, AcpConnection, AcpError, AgentRequest, Pending, Responder};
pub use content::{
    AudioContent, BlobResourceContents, ContentBlock, EmbeddedResource, EmbeddedResourceResource,
    ImageContent, ResourceLink, TextContent, TextResourceContents,
};
pub use files::{ReadTextFileRequest, ReadTextFileResponse, WriteTextFileRequest};
pub use ids::{
    AuthMethodId, ModeId, ModelId, PermissionOptionId, SessionId, TerminalId, ToolCallId,
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
    AvailableCommand, AvailableCommandInput, AvailableCommandsUpdate, ContentChunk,
    CurrentModeUpdate, Diff, Plan, PlanEntry, PlanEntryPriority, PlanEntryStatus,
    SessionNotification, SessionUpdate, ToolCall, ToolCallContent, ToolCallLocation,
    ToolCallStatus, ToolCallUpdate, ToolKind,
};
