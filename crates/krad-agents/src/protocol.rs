mod api;
mod automations;
mod close;
mod harness_name;
mod kraddies;
mod memory;
mod variants;
mod watch;
mod workspace;

pub use api::{ApiError, ApiInfo, ApiVersion, ErrorCode, API_VERSION};
pub use automations::{AutomationInfo, AutomationRunInfo};
pub use close::{
    CloseOrder, CloseTarget, Closed, ClosedTarget, WorktreeFate, WorktreeLeft, WorktreeOutcome,
};
pub(crate) use harness_name::{bus_name, preset_by_bus_name, preset_by_display_name};
pub use krad_domain::{Timestamp, WhenWorking};
pub use kraddies::{KraddieInfo, KraddieMemoryInfo, KraddieRunInfo};
pub use memory::{
    memory_hit_limit, DigestInfo, KraddieChatInfo, KraddieRecallInfo, KraddieSharingInfo,
    MemoryEntryInfo, MemoryHitInfo, MemoryTools, RecallView, MAX_MEMORY_HITS,
};
pub use variants::VariantsRequest;
pub use watch::{
    AgentChangeKind, AgentEvent, AgentFilter, AgentSnapshot, AgentSubscription, CoarseState,
    Heartbeat, StateReason, StreamId,
};
pub use workspace::{ChangeSet, ChangedFile, FilePosition, SessionInfo, WorktreeInfo};

use crate::code::{CodeAnswer, CodeRequest};
use crate::extension_tools::{ExtensionToolAnswer, ToolCatalog};
use crate::handle::Caller;
use krad_domain::{
    AgentPreset, AgentState, PermissionMode, ResetTime, Switch, ToolDecision, ToolUseId,
    UnknownAgentState, VariantRun, WinnerOutcome, ASK_WAIT,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

pub const MAX_WAIT: Duration = Duration::from_mins(30);
const EXITED: &str = "exited";
const CLOSED: &str = "closed";
const BLOCKING_MARGIN: Duration = Duration::from_secs(1);
const DEFAULT_DEADLINE: Duration = Duration::from_secs(10);
const SPAWN_DEADLINE: Duration = Duration::from_mins(1);
const CODE_DEADLINE: Duration = Duration::from_mins(2);
const EXTENSION_TOOL_DEADLINE: Duration = Duration::from_mins(2);
const GIT_DEADLINE: Duration = Duration::from_mins(1);
pub const ASKPASS_WAIT: Duration = Duration::from_mins(5);
const REDACTED: &str = "[REDACTED]";
const NOT_THE_LEAD: &str = "Only the session's lead can start, direct or close workers, close \
                            sessions or pick a variant: ask the user in the lead chat";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentStatus {
    pub model_id: Option<String>,
    pub model_name: Option<String>,
    pub context_used: Option<u16>,
    pub effort: Option<String>,
    pub thinking: Option<bool>,
    pub five_hour_used: Option<u16>,
    pub seven_day_used: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub five_hour_resets_at: Option<ResetTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seven_day_resets_at: Option<ResetTime>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Request {
    List,
    ListAll,
    Send {
        to: u64,
        body: String,
    },
    Check {
        wait_secs: Option<u64>,
    },
    Wait {
        target: u64,
        timeout_secs: u64,
    },
    Read {
        target: u64,
        lines: usize,
    },
    Hook {
        #[serde(with = "harness_name::program")]
        harness: AgentPreset,
        event: String,
        tool: Option<String>,
        detail: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resume: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<PermissionMode>,
    },
    Status(AgentStatus),
    Ask {
        tool: String,
        input: Value,
        #[serde(default)]
        suggestions: Vec<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        call: Option<ToolUseId>,
    },
    Spawn {
        #[serde(with = "harness_name::program")]
        agent: AgentPreset,
        worktree: Option<String>,
        task: String,
    },
    Code(CodeRequest),
    Askpass {
        prompt: String,
    },
    Tools,
    ExtensionTool {
        name: String,
        arguments: Value,
    },
    Hello,
    Sessions,
    CreateSession {
        name: Option<String>,
        group: Option<String>,
    },
    CloseSession {
        session: u64,
    },
    CreateWorktree {
        name: String,
    },
    OpenFile {
        path: PathBuf,
        position: Option<FilePosition>,
    },
    Changes {
        session: Option<u64>,
    },
    Close(CloseOrder),
    Kraddies,
    RunKraddie {
        name: String,
        prompt: Option<String>,
    },
    Automations,
    RunAutomation {
        name: String,
        prompt: Option<String>,
    },
    SwitchAutomation {
        name: String,
        switch: Switch,
    },
    RememberKraddie {
        name: String,
        text: String,
    },
    KraddieMemory {
        name: String,
    },
    KraddieRecall {
        name: String,
        #[serde(default, skip_serializing_if = "RecallView::is_list")]
        view: RecallView,
    },
    NewKraddieChat {
        name: String,
    },
    ShareKraddieMemory {
        reader: String,
        from: String,
        switch: Switch,
    },
    MemorySearch {
        query: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<usize>,
    },
    MemoryRead {
        id: String,
    },
    RunVariants(VariantsRequest),
    Variants {
        run: Option<u64>,
    },
    PickVariant {
        run: u64,
        variant: u32,
        outcome: WinnerOutcome,
        confirmed: bool,
    },
    #[serde(alias = "subscribe_agents")]
    SubscribeAgents(AgentSubscription),
}

impl Request {
    #[must_use]
    pub fn deadline(&self) -> Duration {
        match self {
            Self::Check {
                wait_secs: Some(secs),
            }
            | Self::Wait {
                timeout_secs: secs, ..
            } => Duration::from_secs(*secs).min(MAX_WAIT) + BLOCKING_MARGIN,
            Self::Ask { .. } => ASK_WAIT + BLOCKING_MARGIN,
            Self::List
            | Self::ListAll
            | Self::Send { .. }
            | Self::Check { wait_secs: None }
            | Self::Read { .. }
            | Self::Hook { .. }
            | Self::Status(_)
            | Self::Tools
            | Self::Hello
            | Self::Sessions
            | Self::CreateSession { .. }
            | Self::CloseSession { .. }
            | Self::OpenFile { .. }
            | Self::Kraddies
            | Self::RunKraddie { .. }
            | Self::Automations
            | Self::RunAutomation { .. }
            | Self::SwitchAutomation { .. }
            | Self::RememberKraddie { .. }
            | Self::KraddieMemory { .. }
            | Self::KraddieRecall { .. }
            | Self::NewKraddieChat { .. }
            | Self::ShareKraddieMemory { .. }
            | Self::MemorySearch { .. }
            | Self::MemoryRead { .. }
            | Self::Variants { .. }
            | Self::SubscribeAgents(_) => DEFAULT_DEADLINE,
            Self::CreateWorktree { .. }
            | Self::Changes { .. }
            | Self::Close(_)
            | Self::PickVariant { .. } => GIT_DEADLINE,
            Self::Spawn { .. } | Self::RunVariants(_) => SPAWN_DEADLINE,
            Self::Code(_) => CODE_DEADLINE,
            Self::ExtensionTool { .. } => EXTENSION_TOOL_DEADLINE,
            Self::Askpass { .. } => ASKPASS_WAIT + BLOCKING_MARGIN,
        }
    }

    #[must_use]
    pub const fn directs_workers(&self) -> bool {
        matches!(
            self,
            Self::Spawn { .. }
                | Self::Send { .. }
                | Self::Close(_)
                | Self::CloseSession { .. }
                | Self::PickVariant { .. }
        )
    }

    #[must_use]
    pub const fn followed_terminal(&self) -> Option<u64> {
        match self {
            Self::Read { target, .. } | Self::Wait { target, .. } => Some(*target),
            _ => None,
        }
    }
}

#[must_use]
pub fn lead_refusal(from: &Caller, request: &Request) -> Option<Response> {
    let is_lead = matches!(from, Caller::Lead(_));
    (!is_lead && request.directs_workers())
        .then(|| Response::Error(ApiError::new(ErrorCode::Forbidden, NOT_THE_LEAD)))
}

#[derive(Clone, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BusToken(String);

impl PartialEq for BusToken {
    fn eq(&self, other: &Self) -> bool {
        crate::token::constant_time_eq(self.0.as_bytes(), other.0.as_bytes())
    }
}

impl BusToken {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for BusToken {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for BusToken {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl fmt::Debug for BusToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(REDACTED)
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PromptAnswer(String);

impl PromptAnswer {
    #[must_use]
    pub const fn new(text: String) -> Self {
        Self(text)
    }

    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for PromptAnswer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(REDACTED)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    #[serde(default)]
    pub api_version: ApiVersion,
    pub token: BusToken,
    pub from: Caller,
    pub request: Request,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    pub terminal: u64,
    pub title: String,
    pub group: Option<String>,
    pub state: TerminalState,
    #[serde(with = "harness_name::display_name")]
    pub harness: Option<AgentPreset>,
    pub is_self: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub enum TerminalState {
    Running(AgentState),
    Exited,
    Closed,
}

impl fmt::Display for TerminalState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Running(state) => state.label(),
            Self::Exited => EXITED,
            Self::Closed => CLOSED,
        })
    }
}

impl From<TerminalState> for String {
    fn from(state: TerminalState) -> Self {
        state.to_string()
    }
}

impl TryFrom<String> for TerminalState {
    type Error = UnknownAgentState;

    fn try_from(label: String) -> Result<Self, Self::Error> {
        match label.as_str() {
            EXITED => return Ok(Self::Exited),
            CLOSED => return Ok(Self::Closed),
            _ => {}
        }
        label.parse().map(Self::Running)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub id: u64,
    pub from: u64,
    pub from_title: String,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Response {
    Agents(Vec<AgentInfo>),
    Sent { id: u64 },
    Messages(Vec<Message>),
    Settled { target: u64, state: TerminalState },
    TimedOut,
    Screen(String),
    Noted,
    Spawned { terminal: u64 },
    Submitted { terminal: u64 },
    Decided(ToolDecision),
    Code(CodeAnswer),
    Tools(ToolCatalog),
    ExtensionTool(ExtensionToolAnswer),
    Answered(Option<PromptAnswer>),
    Hello(ApiInfo),
    Sessions(Vec<SessionInfo>),
    SessionCreated { session: u64 },
    SessionClosed { session: u64 },
    WorktreeCreated(Box<WorktreeInfo>),
    Opened { path: PathBuf },
    Changes(Box<ChangeSet>),
    Closed(Vec<Closed>),
    Kraddies(Vec<KraddieInfo>),
    KraddieStarted(KraddieRunInfo),
    Automations(Vec<AutomationInfo>),
    AutomationStarted(AutomationRunInfo),
    AutomationSwitched(Box<AutomationInfo>),
    KraddieMemory(KraddieMemoryInfo),
    KraddieRecall(Box<KraddieRecallInfo>),
    KraddieChatOpened(KraddieChatInfo),
    KraddieShared(KraddieSharingInfo),
    MemoryHits(Vec<MemoryHitInfo>),
    MemoryEntry(Box<MemoryEntryInfo>),
    VariantsStarted { run: u64 },
    Variants(Vec<VariantRun>),
    VariantPicked { run: u64, variant: u32 },
    AgentSnapshot(Box<AgentSnapshot>),
    AgentEvent(Box<AgentEvent>),
    Heartbeat(Heartbeat),
    Error(ApiError),
}

impl Response {
    #[must_use]
    pub fn error(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::Error(ApiError::new(code, message))
    }
}

#[cfg(test)]
mod tests;
