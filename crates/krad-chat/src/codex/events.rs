use krad_domain::{line_diff, ChatEvent, EditLine, FileEdit, LineChange, ToolCall, ToolUseId};
use serde::Deserialize;
use serde_json::Value;

use crate::control::{LiveEvent, PlanUsage};

const BASH_TOOL: &str = "Bash";
const READ_TOOL: &str = "Read";
const LIST_TOOL: &str = "Glob";
const SEARCH_TOOL: &str = "Grep";
const EDIT_TOOL: &str = "Edit";
const WRITE_TOOL: &str = "Write";
const WEB_SEARCH_TOOL: &str = "WebSearch";
const TASK_TOOL: &str = "Task";
const REASONING_SEPARATOR: &str = "\n\n";
const HUNK_MARK: &str = "@@";
const OLD_FILE_MARK: &str = "--- ";
const NEW_FILE_MARK: &str = "+++ ";
const KEPT_MARK: char = ' ';
const REMOVED_MARK: char = '-';
const ADDED_MARK: char = '+';
const FIVE_HOURS_MINS: u32 = 300;
const SEVEN_DAYS_MINS: u32 = 10_080;
const UNKNOWN_FAILURE: &str = "codex reported an error";

const ITEM_STARTED: &str = "item/started";
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
pub(crate) enum ChangeKind {
    Add,
    Delete,
    Update,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FileUpdate {
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) diff: String,
    pub(crate) kind: ChangeKind,
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

fn call(id: &str, name: &str, summary: String, edit: Option<FileEdit>) -> ToolCall {
    ToolCall {
        id: Some(ToolUseId::from(id)),
        name: name.to_owned(),
        summary,
        edit,
    }
}

fn numbered(diff: &str) -> Vec<EditLine> {
    let (mut old, mut new) = (0_u32, 0_u32);
    let mut lines = Vec::new();
    for line in diff.lines() {
        if line.starts_with(OLD_FILE_MARK) || line.starts_with(NEW_FILE_MARK) {
            continue;
        }
        if let Some(header) = line.strip_prefix(HUNK_MARK) {
            if !lines.is_empty() {
                lines.push(EditLine {
                    change: LineChange::Gap,
                    text: String::new(),
                    old_line: None,
                    new_line: None,
                });
            }
            let mut starts = header
                .split_whitespace()
                .filter_map(|range| range.get(1..)?.split(',').next()?.parse::<u32>().ok());
            old = starts.next().unwrap_or(1);
            new = starts.next().unwrap_or(1);
            continue;
        }
        let mut chars = line.chars();
        let Some(mark) = chars.next() else {
            continue;
        };
        let text = chars.as_str().to_owned();
        let (change, old_line, new_line) = match mark {
            KEPT_MARK => (LineChange::Kept, Some(old), Some(new)),
            REMOVED_MARK => (LineChange::Removed, Some(old), None),
            ADDED_MARK => (LineChange::Added, None, Some(new)),
            _ => continue,
        };
        old += u32::from(old_line.is_some());
        new += u32::from(new_line.is_some());
        lines.push(EditLine {
            change,
            text,
            old_line,
            new_line,
        });
    }
    lines
}

pub(crate) fn file_edit(update: &FileUpdate) -> Option<FileEdit> {
    let lines = if update.diff.contains(HUNK_MARK) {
        numbered(&update.diff)
    } else {
        match update.kind {
            ChangeKind::Add => line_diff("", &update.diff),
            ChangeKind::Delete => line_diff(&update.diff, ""),
            ChangeKind::Update => Vec::new(),
        }
    };
    (!lines.is_empty()).then(|| FileEdit {
        path: update.path.clone(),
        lines,
    })
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
        Item::McpToolCall { id, server, tool } => vec![call(id, tool, server.clone(), None)],
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
struct ItemNotice {
    item: Item,
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
                Vec::new()
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
            RATE_LIMITS => parsed::<LimitsNotice>(params)
                .map_or_default(|notice| vec![LiveEvent::Usage(plan_usage(&notice.rate_limits))]),
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn chat(events: Vec<LiveEvent>) -> Vec<ChatEvent> {
        events
            .into_iter()
            .filter_map(|event| match event {
                LiveEvent::Chat(event) => Some(event),
                LiveEvent::Ask(_) | LiveEvent::Ready(_) | LiveEvent::Usage(_) => None,
            })
            .collect()
    }

    #[test]
    fn a_turn_becomes_its_tools_text_usage_and_end() {
        let mut translator = Translator::default();
        let notices = [
            (
                TURN_STARTED,
                json!({"threadId": "t", "turn": {"id": "turn-1", "items": [], "status": "inProgress", "error": null}}),
            ),
            (
                ITEM_STARTED,
                json!({"threadId": "t", "turnId": "turn-1", "item": {"type": "commandExecution", "id": "exec-1",
                "command": "/bin/zsh -lc 'git status'", "status": "inProgress",
                "commandActions": [{"type": "unknown", "command": "git status"}]}}),
            ),
            (
                ITEM_COMPLETED,
                json!({"threadId": "t", "turnId": "turn-1", "item": {"type": "commandExecution", "id": "exec-1",
                "command": "/bin/zsh -lc 'git status'", "status": "completed", "commandActions": []}}),
            ),
            (
                ITEM_COMPLETED,
                json!({"threadId": "t", "turnId": "turn-1", "item": {"type": "fileChange", "id": "patch-1",
                "status": "completed", "changes": [{"path": "src/a.rs", "kind": {"type": "update", "move_path": null},
                "diff": "@@ -3,2 +3,2 @@\n fn a() {}\n-old\n+new\n"}]}}),
            ),
            (
                ITEM_COMPLETED,
                json!({"threadId": "t", "turnId": "turn-1", "item": {"type": "agentMessage", "id": "m1",
                "text": "Clean tree.", "phase": "final_answer"}}),
            ),
            (
                TOKEN_USAGE,
                json!({"threadId": "t", "turnId": "turn-1", "tokenUsage": {
                "total": {"totalTokens": 1, "inputTokens": 1, "cachedInputTokens": 0, "outputTokens": 0, "reasoningOutputTokens": 0},
                "last": {"totalTokens": 14316, "inputTokens": 14263, "cachedInputTokens": 12288, "outputTokens": 53, "reasoningOutputTokens": 0},
                "modelContextWindow": 475_000}}),
            ),
        ];
        let events: Vec<ChatEvent> = notices
            .iter()
            .flat_map(|(method, params)| chat(translator.translate(method, params)))
            .collect();
        assert_eq!(translator.turn.as_deref(), Some("turn-1"));
        let ChatEvent::Tool(edit) = &events[1] else {
            unreachable!("the file change is a tool");
        };
        let lines: Vec<(LineChange, Option<u32>, Option<u32>)> =
            edit.edit.as_ref().map_or_default(|edit| {
                edit.lines
                    .iter()
                    .map(|line| (line.change, line.old_line, line.new_line))
                    .collect()
            });
        assert_eq!(
            lines,
            [
                (LineChange::Kept, Some(3), Some(3)),
                (LineChange::Removed, Some(4), None),
                (LineChange::Added, None, Some(4)),
            ]
        );
        assert_eq!(
            events,
            [
                ChatEvent::Tool(call("exec-1", BASH_TOOL, "git status".to_owned(), None)),
                events[1].clone(),
                ChatEvent::Text("Clean tree.".to_owned()),
                ChatEvent::ContextTokens(14_263),
                ChatEvent::ContextWindow(475_000),
            ]
        );
        let done = translator.translate(
            TURN_COMPLETED,
            &json!({"threadId": "t", "turn": {"id": "turn-1", "items": [], "status": "failed",
                "error": {"message": "usage limit reached"}}}),
        );
        assert_eq!(
            chat(done),
            [ChatEvent::Error("usage limit reached".to_owned())]
        );
        assert_eq!(translator.turn, None);
    }

    #[test]
    fn rate_limits_fill_the_five_hour_and_weekly_rings() {
        let mut translator = Translator::default();
        let events = translator.translate(
            RATE_LIMITS,
            &json!({"rateLimits": {"limitId": "codex",
                "primary": {"usedPercent": 12, "windowDurationMins": 300, "resetsAt": 1},
                "secondary": {"usedPercent": 40, "windowDurationMins": 10_080, "resetsAt": 2}}}),
        );
        assert_eq!(
            events,
            [LiveEvent::Usage(PlanUsage {
                five_hour: Some(12),
                seven_day: Some(40),
            })]
        );
    }
}
