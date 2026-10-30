mod stream;

use krad_domain::ChatImage;
use serde::Serialize;
use serde_json::Value;

use super::{ReasoningEffort, IMAGE_DETAIL};
use crate::engine::attachments::{Attached, Attachments};
use crate::engine::wire::{Block, Message, NativeTool, Request};
pub(super) use stream::Parser;

pub(super) const PATH: &str = "/v1/chat/completions";

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Function,
}

#[derive(Debug, Serialize)]
struct FunctionCall<'a> {
    name: &'a str,
    arguments: String,
}

#[derive(Debug, Serialize)]
struct ToolCall<'a> {
    id: &'a str,
    #[serde(rename = "type")]
    kind: Kind,
    function: FunctionCall<'a>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "role", rename_all = "snake_case")]
enum Turn<'a> {
    System {
        content: &'a str,
    },
    User {
        content: UserContent<'a>,
    },
    Assistant {
        content: Option<String>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        tool_calls: Vec<ToolCall<'a>>,
    },
    Tool {
        tool_call_id: &'a str,
        content: &'a str,
    },
}

#[derive(Debug, Serialize)]
struct ImageUrl {
    url: String,
    detail: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum UserPart<'a> {
    Text { text: &'a str },
    ImageUrl { image_url: ImageUrl },
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum UserContent<'a> {
    Text(&'a str),
    Parts(Vec<UserPart<'a>>),
}

#[derive(Debug, Serialize)]
struct Function<'a> {
    name: &'a str,
    description: &'a str,
    parameters: &'a Value,
}

#[derive(Debug, Serialize)]
struct Empty {}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum Tool<'a> {
    Function {
        #[serde(rename = "type")]
        kind: Kind,
        function: Function<'a>,
    },
    GoogleSearch {
        google_search: Empty,
    },
}

const fn native_tool<'a>(native: NativeTool) -> Tool<'a> {
    match native {
        NativeTool::WebSearch => Tool::GoogleSearch {
            google_search: Empty {},
        },
    }
}

#[derive(Debug, Serialize)]
struct StreamOptions {
    include_usage: bool,
}

#[derive(Debug, Serialize)]
pub(super) struct Body<'a> {
    model: &'a str,
    messages: Vec<Turn<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<Tool<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<ReasoningEffort>,
    max_tokens: u64,
    stream: bool,
    stream_options: StreamOptions,
}

fn assistant_turn(blocks: &[Block]) -> Option<Turn<'_>> {
    let text: String = blocks
        .iter()
        .filter_map(|block| match block {
            Block::Text { text } => Some(text.as_str()),
            Block::Thinking { .. } | Block::Call(_) => None,
        })
        .collect();
    let tool_calls: Vec<ToolCall<'_>> = blocks
        .iter()
        .filter_map(|block| match block {
            Block::Call(call) => Some(ToolCall {
                id: &call.id,
                kind: Kind::Function,
                function: FunctionCall {
                    name: &call.name,
                    arguments: call.arguments.to_string(),
                },
            }),
            Block::Text { .. } | Block::Thinking { .. } => None,
        })
        .collect();
    if text.is_empty() && tool_calls.is_empty() {
        return None;
    }
    Some(Turn::Assistant {
        content: (!text.is_empty()).then_some(text),
        tool_calls,
    })
}

fn attached_part(attached: &Attached) -> UserPart<'_> {
    match attached {
        Attached::Image(image) => UserPart::ImageUrl {
            image_url: ImageUrl {
                url: image.data_url(),
                detail: IMAGE_DETAIL,
            },
        },
        Attached::Unreadable(note) => UserPart::Text { text: note },
    }
}

fn user_content<'a>(
    text: &'a str,
    images: &'a [ChatImage],
    attachments: &'a Attachments,
) -> UserContent<'a> {
    if images.is_empty() {
        return UserContent::Text(text);
    }
    UserContent::Parts(
        attachments
            .of(images)
            .map(attached_part)
            .chain((!text.is_empty()).then_some(UserPart::Text { text }))
            .collect(),
    )
}

fn turns<'a>(
    system: &'a str,
    messages: &'a [Message],
    attachments: &'a Attachments,
) -> Vec<Turn<'a>> {
    let system = (!system.is_empty()).then_some(Turn::System { content: system });
    system
        .into_iter()
        .chain(messages.iter().flat_map(|message| {
            match message {
                Message::User { text, images } => vec![Turn::User {
                    content: user_content(text, images, attachments),
                }],
                Message::Assistant { blocks, .. } => assistant_turn(blocks).into_iter().collect(),
                Message::Results { outcomes } => outcomes
                    .iter()
                    .map(|outcome| Turn::Tool {
                        tool_call_id: &outcome.call,
                        content: &outcome.content,
                    })
                    .collect(),
            }
        }))
        .collect()
}

pub(super) fn body<'a>(request: &Request<'a>) -> Body<'a> {
    Body {
        model: request.model,
        messages: turns(request.system, request.messages, request.images),
        tools: request
            .tools
            .iter()
            .map(|tool| Tool::Function {
                kind: Kind::Function,
                function: Function {
                    name: &tool.name,
                    description: &tool.description,
                    parameters: &tool.parameters,
                },
            })
            .chain(request.native.map(native_tool))
            .collect(),
        reasoning_effort: request.effort.map(ReasoningEffort::from),
        max_tokens: request.max_tokens,
        stream: true,
        stream_options: StreamOptions {
            include_usage: true,
        },
    }
}

#[cfg(test)]
mod tests;
