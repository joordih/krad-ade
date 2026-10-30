use serde::de::IgnoredAny;
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

use super::super::{error_message, object_arguments, push_text, Parse};
use crate::engine::wire::{Block, Call, Delta, Protocol, Reply, Source, Stop, Usage};
use crate::gateway::GatewayError;

const TOOL_USE_STOP: &str = "tool_use";
const LENGTH_STOP: &str = "max_tokens";
const REFUSAL_STOP: &str = "refusal";

#[derive(Default, Deserialize)]
struct StreamUsage {
    #[serde(default, rename = "input_tokens")]
    input: Option<u64>,
    #[serde(default, rename = "output_tokens")]
    output: Option<u64>,
    #[serde(default, rename = "cache_read_input_tokens")]
    cache_read: Option<u64>,
    #[serde(default, rename = "cache_creation_input_tokens")]
    cache_write: Option<u64>,
}

#[derive(Deserialize)]
struct Started {
    #[serde(default)]
    usage: StreamUsage,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StartedBlock {
    Text {
        #[serde(default)]
        text: String,
    },
    Thinking {
        #[serde(default)]
        thinking: String,
        #[serde(default)]
        signature: Option<String>,
    },
    ToolUse {
        id: String,
        name: String,
        #[serde(default)]
        input: Value,
    },
    WebSearchToolResult {
        #[serde(default)]
        content: Option<SearchContent>,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct Hit {
    url: String,
    #[serde(default)]
    title: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum SearchContent {
    Hits(Vec<Hit>),
    Failed(IgnoredAny),
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Citation {
    WebSearchResultLocation {
        url: String,
        #[serde(default)]
        title: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum BlockDelta {
    TextDelta {
        text: String,
    },
    ThinkingDelta {
        thinking: String,
    },
    SignatureDelta {
        signature: String,
    },
    InputJsonDelta {
        partial_json: String,
    },
    CitationsDelta {
        citation: Citation,
    },
    #[serde(other)]
    Other,
}

#[derive(Default, Deserialize)]
struct MessageDelta {
    #[serde(default)]
    stop_reason: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Streamed {
    MessageStart {
        message: Started,
    },
    ContentBlockStart {
        index: usize,
        content_block: StartedBlock,
    },
    ContentBlockDelta {
        index: usize,
        delta: BlockDelta,
    },
    MessageDelta {
        #[serde(default)]
        delta: MessageDelta,
        #[serde(default)]
        usage: StreamUsage,
    },
    Error {
        #[serde(default)]
        error: Option<Failure>,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct Failure {
    message: String,
}

enum Partial {
    Text(String),
    Thinking {
        text: String,
        signature: Option<String>,
    },
    Call {
        id: String,
        name: String,
        input: Value,
        json: String,
    },
}

#[derive(Default)]
pub(in crate::engine::provider) struct Parser {
    blocks: BTreeMap<usize, Partial>,
    usage: Usage,
    stop: Stop,
    sources: Vec<Source>,
}

impl Parser {
    fn take_usage(&mut self, usage: &StreamUsage) {
        if let Some(input) = usage.input {
            self.usage.input = input;
        }
        if let Some(output) = usage.output {
            self.usage.output = output;
        }
        if let Some(read) = usage.cache_read {
            self.usage.cache_read = read;
        }
        if let Some(write) = usage.cache_write {
            self.usage.cache_write = write;
        }
    }

    fn start(&mut self, index: usize, block: StartedBlock) {
        let partial = match block {
            StartedBlock::Text { text } => Partial::Text(text),
            StartedBlock::Thinking {
                thinking,
                signature,
            } => Partial::Thinking {
                text: thinking,
                signature,
            },
            StartedBlock::ToolUse { id, name, input } => Partial::Call {
                id,
                name,
                input,
                json: String::new(),
            },
            StartedBlock::WebSearchToolResult {
                content: Some(SearchContent::Hits(hits)),
            } => {
                self.sources.extend(hits.into_iter().map(|hit| Source {
                    title: hit.title,
                    url: hit.url,
                }));
                return;
            }
            StartedBlock::WebSearchToolResult { .. } | StartedBlock::Other => return,
        };
        self.blocks.insert(index, partial);
    }

    fn extend(&mut self, index: usize, delta: BlockDelta, on_delta: &mut dyn FnMut(Delta)) {
        if let BlockDelta::CitationsDelta {
            citation: Citation::WebSearchResultLocation { url, title },
        } = delta
        {
            return self.sources.push(Source { title, url });
        }
        let Some(partial) = self.blocks.get_mut(&index) else {
            return;
        };
        match (partial, delta) {
            (Partial::Text(text), BlockDelta::TextDelta { text: piece }) => {
                text.push_str(&piece);
                on_delta(Delta::Text(piece));
            }
            (Partial::Thinking { text, .. }, BlockDelta::ThinkingDelta { thinking }) => {
                text.push_str(&thinking);
                on_delta(Delta::Thinking(thinking));
            }
            (
                Partial::Thinking { signature, .. },
                BlockDelta::SignatureDelta { signature: piece },
            ) => {
                signature.get_or_insert_with(String::new).push_str(&piece);
            }
            (Partial::Call { json, .. }, BlockDelta::InputJsonDelta { partial_json }) => {
                json.push_str(&partial_json);
            }
            _ => {}
        }
    }
}

fn stop_of(reason: &str) -> Stop {
    match reason {
        TOOL_USE_STOP => Stop::Tools,
        LENGTH_STOP => Stop::Length,
        REFUSAL_STOP => Stop::Refused,
        _ => Stop::Finished,
    }
}

fn arguments(input: Value, json: &str) -> Value {
    if json.trim().is_empty() {
        return match input {
            Value::Object(_) => input,
            _ => Value::Object(Map::new()),
        };
    }
    object_arguments(json)
}

impl Parse for Parser {
    fn feed(&mut self, data: &str, on_delta: &mut dyn FnMut(Delta)) -> Result<(), GatewayError> {
        match serde_json::from_str::<Streamed>(data)? {
            Streamed::MessageStart { message } => self.take_usage(&message.usage),
            Streamed::ContentBlockStart {
                index,
                content_block,
            } => self.start(index, content_block),
            Streamed::ContentBlockDelta { index, delta } => self.extend(index, delta, on_delta),
            Streamed::MessageDelta { delta, usage } => {
                self.take_usage(&usage);
                if let Some(reason) = delta.stop_reason {
                    self.stop = stop_of(&reason);
                }
            }
            Streamed::Error { error } => {
                let message = error.map(|failure| failure.message);
                return Err(GatewayError::Failed(
                    message.or_else(|| error_message(data)).unwrap_or_default(),
                ));
            }
            Streamed::Other => {}
        }
        Ok(())
    }

    fn finish(self) -> Reply {
        let mut blocks = Vec::with_capacity(self.blocks.len());
        for partial in self.blocks.into_values() {
            match partial {
                Partial::Text(text) => push_text(&mut blocks, &text),
                Partial::Thinking { text, signature } => blocks.push(Block::Thinking {
                    text,
                    signature,
                    protocol: Protocol::Messages,
                }),
                Partial::Call {
                    id,
                    name,
                    input,
                    json,
                } => blocks.push(Block::Call(Call {
                    id,
                    name,
                    arguments: arguments(input, &json),
                })),
            }
        }
        let calls = blocks.iter().any(|block| matches!(block, Block::Call(_)));
        let stop = if calls && self.stop == Stop::Finished {
            Stop::Tools
        } else {
            self.stop
        };
        Reply {
            blocks,
            usage: self.usage,
            stop,
            sources: self.sources,
        }
    }
}
