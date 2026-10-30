mod diff;

use krad_domain::{ChatEvent, FileEdit, RewindId, RewindPoint, ToolCall, ToolKind, ToolUseId};
use serde::Deserialize;
use serde_json::Value;

use super::plan::{checklist, PLAN_UPDATED};
use crate::control::{LiveEvent, PlanUsage};
use diff::file_edit;
pub(crate) use diff::{ChangeKind, FileUpdate};

pub(super) const BASH_TOOL: (&str, ToolKind) = ("Bash", ToolKind::Shell);
const READ_TOOL: (&str, ToolKind) = ("Read", ToolKind::Read);
const LIST_TOOL: (&str, ToolKind) = ("Glob", ToolKind::Search);
const SEARCH_TOOL: (&str, ToolKind) = ("Grep", ToolKind::Search);
pub(super) const EDIT_TOOL: (&str, ToolKind) = ("Edit", ToolKind::Edit);
const WRITE_TOOL: (&str, ToolKind) = ("Write", ToolKind::Edit);
const WEB_SEARCH_TOOL: (&str, ToolKind) = ("WebSearch", ToolKind::Search);
const TASK_TOOL: (&str, ToolKind) = ("Task", ToolKind::Subagent);
const REASONING_SEPARATOR: &str = "\n\n";
const FIVE_HOURS_MINS: u32 = 300;
const SEVEN_DAYS_MINS: u32 = 10_080;
const UNKNOWN_FAILURE: &str = "codex reported an error";

pub(super) const ITEM_STARTED: &str = "item/started";
const ITEM_COMPLETED: &str = "item/completed";
const TURN_STARTED: &str = "turn/started";
const TURN_COMPLETED: &str = "turn/completed";
const TOKEN_USAGE: &str = "thread/tokenUsage/updated";
const SETTINGS_UPDATED: &str = "thread/settings/updated";
const RATE_LIMITS: &str = "account/rateLimits/updated";

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(crate) enum CommandAction {
    Read {
        command: String,
        path: Option<String>,
    },
    ListFiles {
        command: String,
        path: Option<String>,
    },
    Search {
        command: String,
        query: Option<String>,
    },
    Unknown {
        command: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(crate) enum UserContent {
    Text {
        text: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(crate) enum Item {
    UserMessage {
        #[serde(default)]
        content: Vec<UserContent>,
    },
    AgentMessage {
        #[serde(default)]
        text: String,
    },
    Reasoning {
        #[serde(default)]
        summary: Vec<String>,
        #[serde(default)]
        content: Vec<String>,
    },
    #[serde(rename_all = "camelCase")]
    CommandExecution {
        id: String,
        command: String,
        #[serde(default)]
        command_actions: Vec<CommandAction>,
    },
    FileChange {
        id: String,
        #[serde(default)]
        changes: Vec<FileUpdate>,
    },
    McpToolCall {
        id: String,
        server: String,
        tool: String,
    },
    WebSearch {
        id: String,
        #[serde(default)]
        query: String,
    },
    CollabAgentToolCall {
        id: String,
        prompt: Option<String>,
    },
    #[serde(other)]
    Other,
}

fn call(
    id: &str,
    (name, kind): (&str, ToolKind),
    summary: String,
    edit: Option<FileEdit>,
) -> ToolCall {
    ToolCall {
        id: Some(ToolUseId::from(id)),
        name: name.to_owned(),
        kind,
        summary,
        edit,
    }
}

fn command_tool(id: &str, command: &str, actions: &[CommandAction]) -> ToolCall {
    match actions.first() {
        Some(CommandAction::Read { command, path }) => call(
            id,
            READ_TOOL,
            path.clone().unwrap_or_else(|| command.clone()),
            None,
        ),
        Some(CommandAction::ListFiles { command, path }) => call(
            id,
            LIST_TOOL,
            path.clone().unwrap_or_else(|| command.clone()),
            None,
        ),
        Some(CommandAction::Search { command, query }) => call(
            id,
            SEARCH_TOOL,
            query.clone().unwrap_or_else(|| command.clone()),
            None,
        ),
        Some(CommandAction::Unknown { command }) => call(id, BASH_TOOL, command.clone(), None),
        Some(CommandAction::Other) | None => call(id, BASH_TOOL, command.to_owned(), None),
    }
}

pub(crate) fn tools_of(item: &Item) -> Vec<ToolCall> {
    match item {
        Item::CommandExecution {
            id,
            command,
            command_actions,
        } => vec![command_tool(id, command, command_actions)],
        Item::FileChange { id, changes } => changes
            .iter()
            .map(|change| {
                let name = match change.kind {
                    ChangeKind::Add => WRITE_TOOL,
                    ChangeKind::Delete | ChangeKind::Update => EDIT_TOOL,
                };
                call(id, name, change.path.clone(), file_edit(change))
            })
            .collect(),
        Item::McpToolCall { id, server, tool } => {
            vec![call(id, (tool, ToolKind::Other), server.clone(), None)]
        }
        Item::WebSearch { id, query } => vec![call(id, WEB_SEARCH_TOOL, query.clone(), None)],
        Item::CollabAgentToolCall { id, prompt } => {
            vec![call(
                id,
                TASK_TOOL,
                prompt.clone().unwrap_or_default(),
                None,
            )]
        }
        Item::UserMessage { .. }
        | Item::AgentMessage { .. }
        | Item::Reasoning { .. }
        | Item::Other => Vec::new(),
    }
}

pub(crate) fn said(item: &Item) -> Option<ChatEvent> {
    match item {
        Item::AgentMessage { text } if !text.trim().is_empty() => {
            Some(ChatEvent::Text(text.clone()))
        }
        Item::Reasoning { summary, content } => {
            let parts = if summary.is_empty() { content } else { summary };
            let thought = parts.join(REASONING_SEPARATOR);
            let thought = thought.trim();
            (!thought.is_empty()).then(|| ChatEvent::Thinking(thought.to_owned()))
        }
        _ => None,
    }
}

const fn shows_when_started(item: &Item) -> bool {
    matches!(
        item,
        Item::CommandExecution { .. } | Item::McpToolCall { .. } | Item::CollabAgentToolCall { .. }
    )
}

#[derive(Debug, Deserialize)]
pub(super) struct ItemNotice {
    pub(super) item: Item,
}

#[derive(Debug, Deserialize)]
struct TurnInfo {
    id: String,
    status: TurnStatus,
    error: Option<TurnError>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum TurnStatus {
    Completed,
    Interrupted,
    Failed,
    InProgress,
}

#[derive(Debug, Deserialize)]
struct TurnError {
    message: String,
}

#[derive(Debug, Deserialize)]
struct TurnNotice {
    turn: TurnInfo,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Breakdown {
    input_tokens: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TokenUsage {
    last: Breakdown,
    model_context_window: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageNotice {
    token_usage: TokenUsage,
}

#[derive(Debug, Deserialize)]
struct ThreadSettings {
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsNotice {
    thread_settings: ThreadSettings,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LimitWindow {
    used_percent: u16,
    window_duration_mins: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct Limits {
    primary: Option<LimitWindow>,
    secondary: Option<LimitWindow>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LimitsNotice {
    rate_limits: Limits,
}

fn plan_usage(limits: &Limits) -> PlanUsage {
    let used = |minutes: u32| {
        [limits.primary.as_ref(), limits.secondary.as_ref()]
            .into_iter()
            .flatten()
            .find(|window| window.window_duration_mins == Some(minutes))
            .map(|window| window.used_percent)
    };
    PlanUsage {
        five_hour: used(FIVE_HOURS_MINS),
        seven_day: used(SEVEN_DAYS_MINS),
    }
}

fn parsed<T: for<'de> Deserialize<'de>>(params: &Value) -> Option<T> {
    T::deserialize(params).ok()
}

#[derive(Debug, Default)]
pub(crate) struct Translator {
    pub(crate) turn: Option<String>,
    pub(crate) model: Option<String>,
}

impl Translator {
    pub(crate) fn translate(&mut self, method: &str, params: &Value) -> Vec<LiveEvent> {
        let chat = |events: Vec<ChatEvent>| events.into_iter().map(LiveEvent::Chat).collect();
        match method {
            ITEM_STARTED => parsed::<ItemNotice>(params)
                .filter(|notice| shows_when_started(&notice.item))
                .map_or_default(|notice| {
                    chat(
                        tools_of(&notice.item)
                            .into_iter()
                            .map(ChatEvent::Tool)
                            .collect(),
                    )
                }),
            ITEM_COMPLETED => parsed::<ItemNotice>(params).map_or_default(|notice| {
                let tools = if shows_when_started(&notice.item) {
                    Vec::new()
                } else {
                    tools_of(&notice.item)
                };
                chat(
                    said(&notice.item)
                        .into_iter()
                        .chain(tools.into_iter().map(ChatEvent::Tool))
                        .collect(),
                )
            }),
            TURN_STARTED => {
                self.turn = parsed::<TurnNotice>(params).map(|notice| notice.turn.id);
                self.turn.as_deref().map_or_default(|turn| {
                    chat(vec![ChatEvent::Prompted(RewindPoint::Turn(
                        RewindId::from(turn),
                    ))])
                })
            }
            TURN_COMPLETED => {
                self.turn = None;
                parsed::<TurnNotice>(params).map_or_default(|notice| {
                    chat(vec![match notice.turn.status {
                        TurnStatus::Failed => ChatEvent::Error(
                            notice
                                .turn
                                .error
                                .map_or_else(|| UNKNOWN_FAILURE.to_owned(), |error| error.message),
                        ),
                        TurnStatus::Completed
                        | TurnStatus::Interrupted
                        | TurnStatus::InProgress => ChatEvent::Done,
                    }])
                })
            }
            TOKEN_USAGE => parsed::<UsageNotice>(params).map_or_default(|notice| {
                let usage = notice.token_usage;
                chat(
                    std::iter::once(ChatEvent::ContextTokens(usage.last.input_tokens))
                        .chain(usage.model_context_window.map(ChatEvent::ContextWindow))
                        .collect(),
                )
            }),
            SETTINGS_UPDATED => parsed::<SettingsNotice>(params)
                .and_then(|notice| notice.thread_settings.model)
                .filter(|model| self.model.as_ref() != Some(model))
                .map_or_default(|model| {
                    self.model = Some(model.clone());
                    chat(vec![ChatEvent::Model(model)])
                }),
            PLAN_UPDATED => {
                checklist(params).map_or_default(|list| chat(vec![ChatEvent::Checklist(list)]))
            }
            RATE_LIMITS => parsed::<LimitsNotice>(params)
                .map_or_default(|notice| vec![LiveEvent::Usage(plan_usage(&notice.rate_limits))]),
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests;
