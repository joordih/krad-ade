mod stream;

use krad_domain::Effort;
use serde::Serialize;
use serde_json::Value;

use crate::engine::attachments::{Attached, Attachments};
use crate::engine::wire::{Block, Message, NativeTool, Protocol, Request};
pub(super) use stream::Parser;

pub(super) const PATH: &str = "/v1/messages";
pub(super) const HEADERS: [(&str, &str); 1] = [("anthropic-version", "2023-06-01")];
const LOW_BUDGET: u64 = 2_048;
const MEDIUM_BUDGET: u64 = 8_192;
const HIGH_BUDGET: u64 = 16_384;
const EXTRA_HIGH_BUDGET: u64 = 32_768;
const MAX_BUDGET: u64 = 49_152;
const ANSWER_ROOM: u64 = 16_384;
const WEB_SEARCH_KIND: &str = "web_search_20250305";
const WEB_SEARCH_NAME: &str = "web_search";
const WEB_SEARCH_MAX_USES: u32 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Role {
    User,
    Assistant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum SourceKind {
    Base64,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
struct ImageSource<'a> {
    #[serde(rename = "type")]
    kind: SourceKind,
    media_type: &'a str,
    data: &'a str,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Content<'a> {
    Text {
        text: &'a str,
    },
    Image {
        source: ImageSource<'a>,
    },
    Thinking {
        thinking: &'a str,
        signature: &'a str,
    },
    ToolUse {
        id: &'a str,
        name: &'a str,
        input: &'a Value,
    },
    ToolResult {
        tool_use_id: &'a str,
        content: &'a str,
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        is_error: bool,
    },
}

#[derive(Debug, Serialize)]
struct Turn<'a> {
    role: Role,
    content: Vec<Content<'a>>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum Tool<'a> {
    Function {
        name: &'a str,
        description: &'a str,
        input_schema: &'a Value,
    },
    WebSearch {
        #[serde(rename = "type")]
        kind: &'static str,
        name: &'static str,
        max_uses: u32,
    },
}

const fn native_tool<'a>(native: NativeTool) -> Tool<'a> {
    match native {
        NativeTool::WebSearch => Tool::WebSearch {
            kind: WEB_SEARCH_KIND,
            name: WEB_SEARCH_NAME,
            max_uses: WEB_SEARCH_MAX_USES,
        },
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Thinking {
    Enabled { budget_tokens: u64 },
}

#[derive(Debug, Serialize)]
pub(super) struct Body<'a> {
    model: &'a str,
    max_tokens: u64,
    #[serde(skip_serializing_if = "str::is_empty")]
    system: &'a str,
    messages: Vec<Turn<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<Tool<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<Thinking>,
    stream: bool,
}

const fn budget(effort: Effort) -> u64 {
    match effort {
        Effort::Low => LOW_BUDGET,
        Effort::Medium => MEDIUM_BUDGET,
        Effort::High => HIGH_BUDGET,
        Effort::ExtraHigh => EXTRA_HIGH_BUDGET,
        Effort::Max => MAX_BUDGET,
    }
}

fn assistant_content(blocks: &[Block]) -> Vec<Content<'_>> {
    blocks
        .iter()
        .filter_map(|block| match block {
            Block::Text { text } if !text.is_empty() => Some(Content::Text { text }),
            Block::Thinking {
                text,
                signature: Some(signature),
                protocol: Protocol::Messages,
            } => Some(Content::Thinking {
                thinking: text,
                signature,
            }),
            Block::Call(call) => Some(Content::ToolUse {
                id: &call.id,
                name: &call.name,
                input: &call.arguments,
            }),
            Block::Text { .. } | Block::Thinking { .. } => None,
        })
        .collect()
}

fn attached_content(attached: &Attached) -> Content<'_> {
    match attached {
        Attached::Image(image) => Content::Image {
            source: ImageSource {
                kind: SourceKind::Base64,
                media_type: image.media_type,
                data: &image.data,
            },
        },
        Attached::Unreadable(note) => Content::Text { text: note },
    }
}

fn turn_of<'a>(message: &'a Message, attachments: &'a Attachments) -> Turn<'a> {
    match message {
        Message::User { text, images } => Turn {
            role: Role::User,
            content: attachments
                .of(images)
                .map(attached_content)
                .chain((!text.is_empty() || images.is_empty()).then_some(Content::Text { text }))
                .collect(),
        },
        Message::Assistant { blocks, .. } => Turn {
            role: Role::Assistant,
            content: assistant_content(blocks),
        },
        Message::Results { outcomes } => Turn {
            role: Role::User,
            content: outcomes
                .iter()
                .map(|outcome| Content::ToolResult {
                    tool_use_id: &outcome.call,
                    content: &outcome.content,
                    is_error: outcome.failed,
                })
                .collect(),
        },
    }
}

fn turns<'a>(messages: &'a [Message], attachments: &'a Attachments) -> Vec<Turn<'a>> {
    let mut turns: Vec<Turn<'a>> = Vec::with_capacity(messages.len());
    for turn in messages.iter().map(|message| turn_of(message, attachments)) {
        if turn.content.is_empty() {
            continue;
        }
        match turns.last_mut() {
            Some(last) if last.role == turn.role => last.content.extend(turn.content),
            _ => turns.push(turn),
        }
    }
    turns
}

pub(super) fn body<'a>(request: &Request<'a>) -> Body<'a> {
    let thinking = request.effort.map(|effort| Thinking::Enabled {
        budget_tokens: budget(effort),
    });
    let max_tokens = request.effort.map_or(request.max_tokens, |effort| {
        request.max_tokens.max(budget(effort) + ANSWER_ROOM)
    });
    Body {
        model: request.model,
        max_tokens,
        system: request.system,
        messages: turns(request.messages, request.images),
        tools: request
            .tools
            .iter()
            .map(|tool| Tool::Function {
                name: &tool.name,
                description: &tool.description,
                input_schema: &tool.parameters,
            })
            .chain(request.native.map(native_tool))
            .collect(),
        thinking,
        stream: true,
    }
}

#[cfg(test)]
mod tests;
