mod codex;
mod command;
mod control;
mod edits;
mod error;
mod gateway;
mod history;
mod live;
mod message;
mod models;
mod opencode;
mod parse;
mod process;
mod prompt;
mod skills;
mod subagents;
mod turn;

pub use codex::{codex_models, read_codex_transcript, CodexError};
pub use command::{
    chat_command, live_chat_command, supports_chat, supports_live_chat, supports_orchestration,
    BusVars, ChatCommand, LiveSession, Orchestration,
};
pub use control::{
    Approvals, AskAnswer, AskKind, Catalog, CommandOffer, LiveEvent, ModelOffer, ModelSource,
    PickedAnswer, PlanUsage, Question, QuestionOption, ToolAsk,
};
pub use error::ChatError;
pub use gateway::{gateway_env, gateway_models, GatewayError};
pub use history::{
    claude_conversation_file, read_claude_transcript, read_claude_transcript_before,
    read_claude_transcript_tail, TranscriptChunk,
};
pub use krad_domain::{ChatEvent, Effort, ToolCall, ToolUseId};
pub use live::{
    is_missing_conversation, new_session_id, spawn_codex_chat, spawn_live_chat,
    spawn_opencode_chat, LiveChat, LiveConfig, MessageQueue,
};
pub use message::SlashCommand;
pub use models::claude_models;
pub use opencode::{
    new_opencode_session_id, opencode_models, prepare_opencode_session, read_opencode_transcript,
    watch_opencode_session, OpencodeError, OpencodeServer,
};
pub use parse::parse_line;
pub use process::Runner;
pub use prompt::{compose_prompt, shared_skill_message, ChatContext, PriorMessage, Speaker};
pub use skills::{claude_skills, search_skills, Skill, SkillSource};
pub use subagents::{
    claude_subagents, read_subagent_meta, read_subagent_record, SubagentFiles, SubagentMeta,
};
pub use turn::{spawn_turn, ChatRequest, TurnHandle};
