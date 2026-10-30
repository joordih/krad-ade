mod stream;

use krad_domain::Effort;
use serde::Serialize;
use serde_json::Value;

use crate::engine::attachments::{Attached, Attachments};
use crate::engine::wire::{Block, Message, NativeTool, Protocol, Request};
pub(super) use stream::Parser;

pub(super) const PATH: &str = "/v1/responses";
const ENCRYPTED_REASONING: &str = "reasoning.encrypted_content";
const AUTO_SUMMARY: &str = "auto";
const LOW_EFFORT: &str = "low";
const MEDIUM_EFFORT: &str = "medium";
const HIGH_EFFORT: &str = "high";
const IMAGE_DETAIL: &str = "auto";

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Part<'a> {
    InputText {
        text: &'a str,
    },
    InputImage {
        image_url: String,
        detail: &'static str,
    },
    OutputText {
        text: &'a str,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Summary<'a> {
    SummaryText { text: &'a str },
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum Role {
    User,
    Assistant,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Item<'a> {
    Message {
        role: Role,
        content: Vec<Part<'a>>,
    },
    Reasoning {
        encrypted_content: &'a str,
        summary: Vec<Summary<'a>>,
    },
    FunctionCall {
        call_id: &'a str,
        name: &'a str,
        arguments: String,
    },
    FunctionCallOutput {
        call_id: &'a str,
        output: &'a str,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Tool<'a> {
    Function {
        name: &'a str,
        description: &'a str,
        parameters: &'a Value,
    },
    WebSearch,
}

const fn native_tool<'a>(native: NativeTool) -> Tool<'a> {
    match native {
        NativeTool::WebSearch => Tool::WebSearch,
    }
}

#[derive(Debug, Serialize)]
struct Reasoning {
    effort: &'static str,
    summary: &'static str,
}

#[derive(Debug, Serialize)]
pub(super) struct Body<'a> {
    model: &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    instructions: &'a str,
    input: Vec<Item<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<Tool<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<Reasoning>,
    include: [&'static str; 1],
    max_output_tokens: u64,
    store: bool,
    stream: bool,
}

const fn effort_name(effort: Effort) -> &'static str {
    match effort {
        Effort::Low => LOW_EFFORT,
        Effort::Medium => MEDIUM_EFFORT,
        Effort::High | Effort::ExtraHigh | Effort::Max => HIGH_EFFORT,
    }
}

fn assistant_items(blocks: &[Block]) -> Vec<Item<'_>> {
    blocks
        .iter()
        .filter_map(|block| match block {
            Block::Text { text } if !text.is_empty() => Some(Item::Message {
                role: Role::Assistant,
                content: vec![Part::OutputText { text }],
            }),
            Block::Thinking {
                text,
                signature: Some(signature),
                protocol: Protocol::Responses,
            } => Some(Item::Reasoning {
                encrypted_content: signature,
                summary: if text.is_empty() {
                    Vec::new()
                } else {
                    vec![Summary::SummaryText { text }]
                },
            }),
            Block::Call(call) => Some(Item::FunctionCall {
                call_id: &call.id,
                name: &call.name,
                arguments: call.arguments.to_string(),
            }),
            Block::Text { .. } | Block::Thinking { .. } => None,
        })
        .collect()
}

fn attached_part(attached: &Attached) -> Part<'_> {
    match attached {
        Attached::Image(image) => Part::InputImage {
            image_url: image.data_url(),
            detail: IMAGE_DETAIL,
        },
        Attached::Unreadable(note) => Part::InputText { text: note },
    }
}

fn items<'a>(messages: &'a [Message], attachments: &'a Attachments) -> Vec<Item<'a>> {
    messages
        .iter()
        .flat_map(|message| match message {
            Message::User { text, images } => vec![Item::Message {
                role: Role::User,
                content: attachments
                    .of(images)
                    .map(attached_part)
                    .chain(
                        (!text.is_empty() || images.is_empty()).then_some(Part::InputText { text }),
                    )
                    .collect(),
            }],
            Message::Assistant { blocks, .. } => assistant_items(blocks),
            Message::Results { outcomes } => outcomes
                .iter()
                .map(|outcome| Item::FunctionCallOutput {
                    call_id: &outcome.call,
                    output: &outcome.content,
                })
                .collect(),
        })
        .collect()
}

pub(super) fn body<'a>(request: &Request<'a>) -> Body<'a> {
    Body {
        model: request.model,
        instructions: request.system,
        input: items(request.messages, request.images),
        tools: request
            .tools
            .iter()
            .map(|tool| Tool::Function {
                name: &tool.name,
                description: &tool.description,
                parameters: &tool.parameters,
            })
            .chain(request.native.map(native_tool))
            .collect(),
        reasoning: request.effort.map(|effort| Reasoning {
            effort: effort_name(effort),
            summary: AUTO_SUMMARY,
        }),
        include: [ENCRYPTED_REASONING],
        max_output_tokens: request.max_tokens,
        store: false,
        stream: true,
    }
}

#[cfg(test)]
mod tests;
