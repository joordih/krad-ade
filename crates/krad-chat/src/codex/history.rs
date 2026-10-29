use krad_domain::{AgentPreset, ChatEntry, ChatEvent, ResumeId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::sync::mpsc::channel;

use super::events::{said, tools_of, Item, UserContent};
use super::rpc::{AppServer, CodexError};
use crate::process::Runner;

const TURNS_LIST: &str = "thread/turns/list";
const FULL_ITEMS: &str = "full";
const ASCENDING: &str = "asc";
const PAGE_SIZE: u32 = 50;
const MAX_PAGES: usize = 200;
const TEXT_SEPARATOR: &str = "\n";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TurnsList<'a> {
    thread_id: &'a str,
    items_view: &'static str,
    sort_direction: &'static str,
    limit: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Turn {
    #[serde(default)]
    items: Vec<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TurnsPage {
    #[serde(default)]
    data: Vec<Turn>,
    next_cursor: Option<String>,
}

fn entries_of(item: &Item) -> Vec<ChatEntry> {
    if let Item::UserMessage { content } = item {
        let text: Vec<&str> = content
            .iter()
            .filter_map(|part| match part {
                UserContent::Text { text } => Some(text.as_str()),
                UserContent::Other => None,
            })
            .collect();
        let text = text.join(TEXT_SEPARATOR);
        return (!text.trim().is_empty())
            .then(|| ChatEntry::User {
                text: text.trim().to_owned(),
                preset: AgentPreset::Codex,
                model: None,
            })
            .into_iter()
            .collect();
    }
    said(item)
        .into_iter()
        .chain(tools_of(item).into_iter().map(ChatEvent::Tool))
        .map(ChatEntry::Event)
        .collect()
}

fn entries(turns: Vec<Turn>) -> Vec<ChatEntry> {
    turns
        .into_iter()
        .flat_map(|turn| turn.items)
        .filter_map(|item| Item::deserialize(item).ok())
        .flat_map(|item| entries_of(&item))
        .collect()
}

/// # Errors
///
/// Fails when codex cannot be started or cannot read the conversation.
pub fn read_codex_transcript(
    runner: &Runner,
    cwd: &Path,
    resume: &ResumeId,
) -> Result<Vec<ChatEntry>, CodexError> {
    let (pushed, _pushes) = channel();
    let server = AppServer::start(runner, cwd, &[], &[], pushed)?;
    let mut turns = Vec::new();
    let mut cursor = None;
    for _ in 0..MAX_PAGES {
        let page: TurnsPage = server.call(
            TURNS_LIST,
            &TurnsList {
                thread_id: resume.as_str(),
                items_view: FULL_ITEMS,
                sort_direction: ASCENDING,
                limit: PAGE_SIZE,
                cursor: cursor.take(),
            },
        )?;
        let empty = page.data.is_empty();
        turns.extend(page.data);
        match page.next_cursor {
            Some(next) if !empty => cursor = Some(next),
            _ => break,
        }
    }
    Ok(entries(turns))
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::ToolCall;
    use serde_json::json;

    #[test]
    fn a_stored_thread_reads_back_as_the_chat_showed_it() {
        let turns: Vec<Turn> = serde_json::from_value(json!([{"items": [
            {"type": "userMessage", "id": "u1", "content": [{"type": "text", "text": "Read the notes", "text_elements": []}]},
            {"type": "reasoning", "id": "r1", "summary": ["Look first"], "content": []},
            {"type": "commandExecution", "id": "exec-1", "command": "/bin/zsh -lc 'cat notes.txt'", "status": "completed",
             "commandActions": [{"type": "read", "command": "cat notes.txt", "name": "notes.txt", "path": "/p/notes.txt"}]},
            {"type": "agentMessage", "id": "m1", "text": "They say hello."}
        ]}]))
        .unwrap_or_else(|_| unreachable!("the fixture is valid"));
        assert_eq!(
            entries(turns),
            [
                ChatEntry::User {
                    text: "Read the notes".to_owned(),
                    preset: AgentPreset::Codex,
                    model: None,
                },
                ChatEntry::Event(ChatEvent::Thinking("Look first".to_owned())),
                ChatEntry::Event(ChatEvent::Tool(ToolCall {
                    id: Some("exec-1".into()),
                    name: "Read".to_owned(),
                    summary: "/p/notes.txt".to_owned(),
                    edit: None,
                })),
                ChatEntry::Event(ChatEvent::Text("They say hello.".to_owned())),
            ]
        );
    }
}
