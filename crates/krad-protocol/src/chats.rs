use krad_domain::{
    AcpAuthMethod, AcpAuthMethodId, AcpConfigOption, AcpConfigOptionId, AcpConfigValue, AcpModeId,
    AcpModels, AcpModes, AgentPreset, AskAnswer, AskKind, ChatMessage, ChatOrigin, CommandOffer,
    Effort, EnvironmentName, ImageFormat, McpProjectTrust, ModelId, PermissionMode, PickedAnswer,
    Prompt, ResumeId, RewindScope,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

use crate::bus::AgentRef;
use crate::event::EventKind;

mod history;
mod mcp_trust;
mod mirror;
mod open;
mod transcript;

pub use history::{HistoryAsk, HistoryCursor, HistoryPart};
pub use mcp_trust::{McpOffered, McpSkippedShown, McpTrustAsk};
pub use mirror::{ChatMirror, MirrorChange};
pub use open::ChatOpen;
pub use transcript::{
    KnownRecord, SubagentUpdate, SubagentsAsk, SubagentsFrom, SubagentsRead, TranscriptAsk,
    TranscriptRead,
};

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChatAskId(pub u64);

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum ChatCommand {
    Open {
        chat: AgentRef,
        start: Box<ChatOpen>,
    },
    Close {
        chat: AgentRef,
    },
    Page {
        chat: AgentRef,
        before: usize,
        count: usize,
    },
    Mentions {
        environment: EnvironmentName,
        root: PathBuf,
        query: String,
        limit: usize,
    },
    ForgetMentions,
    Transcript {
        environment: EnvironmentName,
        ask: TranscriptAsk,
    },
    Subagents {
        environment: EnvironmentName,
        ask: SubagentsAsk,
    },
    History {
        environment: EnvironmentName,
        ask: HistoryAsk,
    },
    Send {
        chat: AgentRef,
        prompt: Prompt,
    },
    Queue {
        chat: AgentRef,
        prompt: Prompt,
    },
    Unqueue {
        chat: AgentRef,
        index: usize,
    },
    ShowEarlier {
        chat: AgentRef,
    },
    Interrupt {
        chat: AgentRef,
    },
    Rewind {
        chat: AgentRef,
        index: usize,
        prompt: Prompt,
        scope: RewindScope,
    },
    SetModel {
        chat: AgentRef,
        model: ModelId,
    },
    SetEffort {
        chat: AgentRef,
        effort: Effort,
    },
    SetPermissionMode {
        chat: AgentRef,
        mode: PermissionMode,
    },
    SetThinking {
        chat: AgentRef,
        thinking: bool,
    },
    AnswerAsk {
        chat: AgentRef,
        ask: ChatAskId,
        answer: AskAnswer,
    },
    AnswerQuestion {
        chat: AgentRef,
        ask: ChatAskId,
        answers: Vec<PickedAnswer>,
    },
    ApprovePlan {
        chat: AgentRef,
        ask: ChatAskId,
        mode: PermissionMode,
    },
    SetAcpMode {
        chat: AgentRef,
        mode: AcpModeId,
    },
    SetAcpOption {
        chat: AgentRef,
        option: AcpConfigOptionId,
        value: AcpConfigValue,
    },
    SignIn {
        chat: AgentRef,
        method: AcpAuthMethodId,
    },
    TrustMcp {
        chat: AgentRef,
        trust: McpProjectTrust,
    },
    AttachImage {
        chat: AgentRef,
        image: PastedImage,
    },
    ForgetImages {
        chat: AgentRef,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct PastedImage {
    pub format: ImageFormat,
    pub bytes: Vec<u8>,
}

impl ChatCommand {
    #[must_use]
    pub const fn chat(&self) -> Option<&AgentRef> {
        match self {
            Self::Open { chat, .. }
            | Self::Close { chat }
            | Self::Page { chat, .. }
            | Self::Send { chat, .. }
            | Self::Queue { chat, .. }
            | Self::Unqueue { chat, .. }
            | Self::ShowEarlier { chat }
            | Self::Interrupt { chat }
            | Self::Rewind { chat, .. }
            | Self::SetModel { chat, .. }
            | Self::SetEffort { chat, .. }
            | Self::SetPermissionMode { chat, .. }
            | Self::SetThinking { chat, .. }
            | Self::AnswerAsk { chat, .. }
            | Self::AnswerQuestion { chat, .. }
            | Self::ApprovePlan { chat, .. }
            | Self::SetAcpMode { chat, .. }
            | Self::SetAcpOption { chat, .. }
            | Self::SignIn { chat, .. }
            | Self::TrustMcp { chat, .. }
            | Self::AttachImage { chat, .. }
            | Self::ForgetImages { chat } => Some(chat),
            Self::Mentions { .. }
            | Self::ForgetMentions
            | Self::Transcript { .. }
            | Self::Subagents { .. }
            | Self::History { .. }
            | Self::Unknown => None,
        }
    }
}

impl EventKind {
    #[must_use]
    pub const fn chat(&self) -> Option<&AgentRef> {
        match self {
            Self::ChatOpened { chat, .. }
            | Self::ChatMessageAdded { chat, .. }
            | Self::ChatMessageChanged { chat, .. }
            | Self::ChatMessageAppended { chat, .. }
            | Self::ChatMessagesCut { chat, .. }
            | Self::ChatMessagesPrepended { chat, .. }
            | Self::ChatCheckpointed { chat, .. }
            | Self::ChatState { chat, .. }
            | Self::ChatAskSettled { chat, .. }
            | Self::ChatClosed { chat }
            | Self::ChatSpent { chat, .. } => Some(chat),
            Self::ChatAsk(ask) => Some(&ask.chat),
            _ => None,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct ChatContext {
    pub tokens: Option<u64>,
    pub window: Option<u64>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct AcpSignInAsk {
    pub methods: Vec<AcpAuthMethod>,
    pub asked: u32,
}

#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct AcpShown {
    pub modes: Option<AcpModes>,
    pub models: Option<AcpModels>,
    #[serde(default)]
    pub options: Vec<AcpConfigOption>,
    #[serde(default)]
    pub reported: bool,
    #[serde(default)]
    pub sign_in: Option<AcpSignInAsk>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ChatState {
    pub harness: AgentPreset,
    pub working: bool,
    pub alive: bool,
    pub resume: Option<ResumeId>,
    pub model: Option<String>,
    pub chosen_model: Option<ModelId>,
    pub effort: Option<Effort>,
    pub permission_mode: Option<PermissionMode>,
    pub context: ChatContext,
    pub queued: Vec<Prompt>,
    pub problem: Option<String>,
    #[serde(default)]
    pub notices: Vec<String>,
    #[serde(default)]
    pub working_since: Option<SystemTime>,
    #[serde(default)]
    pub earlier: bool,
    #[serde(default)]
    pub origin: Option<ChatOrigin>,
    #[serde(default)]
    pub commands: Vec<CommandOffer>,
    #[serde(default)]
    pub account: Option<String>,
    #[serde(default)]
    pub acp: Option<AcpShown>,
    #[serde(default)]
    pub mcp_trust: Option<McpTrustAsk>,
}

impl ChatState {
    #[must_use]
    pub const fn new(harness: AgentPreset) -> Self {
        Self {
            harness,
            working: false,
            alive: false,
            resume: None,
            model: None,
            chosen_model: None,
            effort: None,
            permission_mode: None,
            context: ChatContext {
                tokens: None,
                window: None,
            },
            queued: Vec::new(),
            problem: None,
            notices: Vec::new(),
            working_since: None,
            earlier: false,
            origin: None,
            commands: Vec::new(),
            account: None,
            acp: None,
            mcp_trust: None,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ChatAsk {
    pub chat: AgentRef,
    pub ask: ChatAskId,
    pub kind: AskKind,
    pub can_allow_always: bool,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ChatPage {
    pub chat: AgentRef,
    pub len: usize,
    pub from: usize,
    pub messages: Vec<ChatMessage>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ChatSummary {
    pub state: ChatState,
    pub tail: ChatPage,
    pub asks: Vec<ChatAsk>,
}

#[cfg(test)]
mod tests;
