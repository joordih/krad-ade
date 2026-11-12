mod askpass;
mod cli;
mod client;
mod code;
mod delivery;
mod discovery;
mod env;
mod error;
mod extension_tools;
mod feed;
mod handle;
mod hook;
mod host_verbs;
mod mailbox;
mod mcp;
mod offline;
mod protocol;
mod server;
mod statusline;
mod token;
mod toolchains;

pub use askpass::ASKPASS_VERB;
pub use cli::{run_cli, run_invoked, run_process, CLI_NAME};
pub use client::{call, watch};
pub use code::{CodeAnswer, CodeError, CodeLine, CodePosition, CodeRequest, CodeTool};
pub use discovery::{liveness, ApiDiscovery, ApiFile, Liveness, API_FILE_NAME};
pub use env::{
    endpoint_file_env, guide, member_token, orchestrator_guide, terminal_env, token_file_env,
    CLI_PROGRAM, ENV_ENDPOINT, ENV_ENDPOINT_FILE, ENV_TERMINAL, ENV_TOKEN, ENV_TOKEN_FILE,
};
pub use error::{AgentsError, HandleError};
pub use extension_tools::{
    extension_tool_name, is_extension_tool, ExtensionToolAnswer, ExtensionToolEffect,
    ExtensionToolInfo, ExtensionToolOutcome, ToolCatalog, EXTENSION_TOOL_PREFIX,
};
pub use feed::{AgentFeed, AgentKey, EVENT_RING_CAPACITY, SUBSCRIBER_QUEUE};
pub use handle::{Caller, LeadHandle, TerminalHandle};
pub use mailbox::{notice_text, Mailbox};
pub use offline::{queued_events, QueuedEvent};
pub use protocol::{
    lead_refusal, memory_hit_limit, AgentChangeKind, AgentEvent, AgentFilter, AgentInfo,
    AgentSnapshot, AgentStatus, AgentSubscription, ApiError, ApiInfo, ApiVersion, AutomationInfo,
    AutomationRunInfo, BusToken, ChangeSet, ChangedFile, CloseOrder, CloseTarget, Closed,
    ClosedTarget, CoarseState, DigestInfo, Envelope, ErrorCode, FilePosition, Heartbeat,
    KraddieChatInfo, KraddieInfo, KraddieMemoryInfo, KraddieRecallInfo, KraddieRunInfo,
    KraddieSharingInfo, MemoryEntryInfo, MemoryHitInfo, MemoryTools, Message, PromptAnswer,
    RecallView, Request, Response, SessionInfo, StateReason, StreamId, TerminalState, Timestamp,
    VariantsRequest, WhenWorking, WorktreeFate, WorktreeInfo, WorktreeLeft, WorktreeOutcome,
    API_VERSION, ASKPASS_WAIT, MAX_WAIT,
};
pub use server::{serve, Endpoint, Incoming, HEARTBEAT_INTERVAL};
pub use token::new_token;
