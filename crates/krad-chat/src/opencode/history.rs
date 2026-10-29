use krad_domain::{AgentPreset, ChatEntry, ChatEvent, ResumeId};
use serde::Deserialize;
use serde_json::Value;

use super::chat::opencode_session_id;
use super::events::tool_call;
use super::prompt::strip_preamble;
use super::server::{OpencodeError, OpencodeServer};

const FIRST_PAGE_QUERY: &str = "/message?limit=200&order=asc";
const NEXT_PAGE_QUERY: &str = "/message?limit=200&cursor=";
const MAX_PAGES: usize = 50;

#[derive(Debug, Default, Deserialize)]
struct Cursor {
    next: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Page {
    #[serde(default)]
    data: Vec<Value>,
    #[serde(default)]
    cursor: Cursor,
}

#[derive(Debug, Default, Deserialize)]
struct ToolState {
    #[serde(default)]
    input: Value,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Content {
    Text {
        #[serde(default)]
        text: String,
    },
    Reasoning {
        #[serde(default)]
        text: String,
    },
    Tool {
        id: String,
        name: String,
        #[serde(default)]
        state: ToolState,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Message {
    User {
        #[serde(default)]
        text: String,
    },
    Assistant {
        #[serde(default)]
        content: Vec<Content>,
    },
    #[serde(other)]
    Other,
}

fn entries_of(message: Message) -> Vec<ChatEntry> {
    match message {
        Message::User { text } => {
            let text = strip_preamble(&text).trim();
            if text.is_empty() {
                return Vec::new();
            }
            vec![ChatEntry::User {
                text: text.to_owned(),
                preset: AgentPreset::Opencode,
                model: None,
            }]
        }
        Message::Assistant { content } => content
            .into_iter()
            .filter_map(|part| match part {
                Content::Text { text } if !text.trim().is_empty() => Some(ChatEvent::Text(text)),
                Content::Reasoning { text } if !text.trim().is_empty() => {
                    Some(ChatEvent::Thinking(text.trim().to_owned()))
                }
                Content::Tool { id, name, state } => {
                    Some(ChatEvent::Tool(tool_call(&id, &name, &state.input)))
                }
                Content::Text { .. } | Content::Reasoning { .. } | Content::Other => None,
            })
            .map(ChatEntry::Event)
            .collect(),
        Message::Other => Vec::new(),
    }
}

fn entries(messages: Vec<Value>) -> Vec<ChatEntry> {
    messages
        .into_iter()
        .filter_map(|message| Message::deserialize(message).ok())
        .flat_map(entries_of)
        .collect()
}

/// # Errors
///
/// Fails when the server cannot read the conversation.
pub fn read_opencode_transcript(
    server: &OpencodeServer,
    resume: &ResumeId,
) -> Result<Vec<ChatEntry>, OpencodeError> {
    let session = format!("/api/session/{}", opencode_session_id(resume));
    let mut messages = Vec::new();
    let mut path = format!("{session}{FIRST_PAGE_QUERY}");
    for _ in 0..MAX_PAGES {
        let page: Page = server.read(&path)?;
        let empty = page.data.is_empty();
        messages.extend(page.data);
        match page.cursor.next {
            Some(next) if !empty => path = format!("{session}{NEXT_PAGE_QUERY}{next}"),
            _ => break,
        }
    }
    Ok(entries(messages))
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::ToolCall;
    use serde_json::json;

    #[test]
    fn a_stored_conversation_reads_back_as_the_chat_showed_it() {
        let messages = vec![
            json!({"type": "agent-switched", "agent": "build"}),
            json!({"type": "user", "id": "msg_1", "text": "<system-reminder>\nUse the bus.\n</system-reminder>\n\nList the files"}),
            json!({"type": "assistant", "id": "msg_2", "content": [
                {"type": "reasoning", "text": " first look "},
                {"type": "tool", "id": "call_1", "name": "glob", "state": {"status": "completed", "input": {"pattern": "*.rs"}}},
                {"type": "text", "text": "Two files."}
            ]}),
        ];
        assert_eq!(
            entries(messages),
            [
                ChatEntry::User {
                    text: "List the files".to_owned(),
                    preset: AgentPreset::Opencode,
                    model: None,
                },
                ChatEntry::Event(ChatEvent::Thinking("first look".to_owned())),
                ChatEntry::Event(ChatEvent::Tool(ToolCall {
                    id: Some("call_1".into()),
                    name: "Glob".to_owned(),
                    summary: "*.rs".to_owned(),
                    edit: None,
                })),
                ChatEntry::Event(ChatEvent::Text("Two files.".to_owned())),
            ]
        );
    }
}
