use krad_domain::{ChatImage, Effort};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::borrow::Cow;

use super::attachments::Attachments;
use crate::gateway::Vendor;

const REASONING_EFFORTS: [Effort; 3] = [Effort::Low, Effort::Medium, Effort::High];
const CUT_OFF_TEXT: &str = "The answer was cut off: it reached the output token limit.";
const REFUSED_TEXT: &str = "The model refused to answer.";
const BLOCK_JOINER: &str = "\n";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Protocol {
    Messages,
    Responses,
    Completions,
}

impl From<Vendor> for Protocol {
    fn from(vendor: Vendor) -> Self {
        match vendor {
            Vendor::Anthropic => Self::Messages,
            Vendor::OpenAi => Self::Responses,
            Vendor::Google | Vendor::Other => Self::Completions,
        }
    }
}

impl Protocol {
    pub(crate) const fn efforts(self) -> &'static [Effort] {
        match self {
            Self::Messages => &Effort::ALL,
            Self::Responses | Self::Completions => &REASONING_EFFORTS,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum Block {
    Text {
        text: String,
    },
    Thinking {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
        protocol: Protocol,
    },
    Call(Call),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Call {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) arguments: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Outcome {
    pub(crate) call: String,
    pub(crate) content: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) failed: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Usage {
    pub(crate) input: u64,
    pub(crate) output: u64,
    pub(crate) cache_read: u64,
    pub(crate) cache_write: u64,
}

impl Usage {
    pub(crate) const fn context(self) -> u64 {
        self.input + self.cache_read + self.cache_write + self.output
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Stop {
    #[default]
    Finished,
    Tools,
    Length,
    Refused,
}

impl Stop {
    pub(crate) const fn problem(self) -> Option<&'static str> {
        match self {
            Self::Finished | Self::Tools => None,
            Self::Length => Some(CUT_OFF_TEXT),
            Self::Refused => Some(REFUSED_TEXT),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub(crate) enum Message {
    User {
        text: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        images: Vec<ChatImage>,
    },
    Assistant {
        blocks: Vec<Block>,
        #[serde(default)]
        usage: Usage,
        #[serde(default)]
        stop: Stop,
    },
    Results {
        outcomes: Vec<Outcome>,
    },
}

impl Message {
    pub(crate) const fn user(text: String) -> Self {
        Self::User {
            text,
            images: Vec::new(),
        }
    }

    pub(crate) fn images(&self) -> &[ChatImage] {
        match self {
            Self::User { images, .. } => images,
            Self::Assistant { .. } | Self::Results { .. } => &[],
        }
    }
}

pub(crate) fn answer_text(blocks: &[Block]) -> String {
    blocks
        .iter()
        .filter_map(|block| match block {
            Block::Text { text } => Some(text.as_str()),
            Block::Thinking { .. } | Block::Call(_) => None,
        })
        .collect::<Vec<_>>()
        .join(BLOCK_JOINER)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ToolSpec {
    pub(crate) name: Cow<'static, str>,
    pub(crate) description: Cow<'static, str>,
    pub(crate) parameters: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeTool {
    WebSearch,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Request<'a> {
    pub(crate) model: &'a str,
    pub(crate) system: &'a str,
    pub(crate) messages: &'a [Message],
    pub(crate) images: &'a Attachments,
    pub(crate) tools: &'a [ToolSpec],
    pub(crate) native: Option<NativeTool>,
    pub(crate) effort: Option<Effort>,
    pub(crate) max_tokens: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Delta {
    Text(String),
    Thinking(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Source {
    pub(crate) title: String,
    pub(crate) url: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Reply {
    pub(crate) blocks: Vec<Block>,
    pub(crate) usage: Usage,
    pub(crate) stop: Stop,
    pub(crate) sources: Vec<Source>,
}
