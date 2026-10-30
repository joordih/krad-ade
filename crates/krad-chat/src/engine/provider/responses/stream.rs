use serde::Deserialize;

use super::super::{error_message, object_arguments, push_text, Parse};
use crate::engine::wire::{Block, Call, Delta, Protocol, Reply, Source, Stop, Usage};
use crate::gateway::GatewayError;

const LENGTH_REASON: &str = "max_output_tokens";
const SUMMARY_JOINER: &str = "\n\n";

#[derive(Deserialize)]
struct SummaryText {
    #[serde(default)]
    text: String,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OutputPart {
    OutputText {
        #[serde(default)]
        text: String,
        #[serde(default)]
        annotations: Vec<Annotation>,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Annotation {
    UrlCitation {
        url: String,
        #[serde(default)]
        title: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OutputItem {
    Message {
        #[serde(default)]
        content: Vec<OutputPart>,
    },
    Reasoning {
        #[serde(default)]
        summary: Vec<SummaryText>,
        #[serde(default)]
        encrypted_content: Option<String>,
    },
    FunctionCall {
        call_id: String,
        name: String,
        #[serde(default)]
        arguments: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Default, Deserialize)]
struct InputDetails {
    #[serde(default)]
    cached_tokens: u64,
}

#[derive(Default, Deserialize)]
struct ResponseUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    input_tokens_details: InputDetails,
}

#[derive(Default, Deserialize)]
struct Incomplete {
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Default, Deserialize)]
struct Failure {
    #[serde(default)]
    message: Option<String>,
}

#[derive(Default, Deserialize)]
struct Finished {
    #[serde(default)]
    usage: Option<ResponseUsage>,
    #[serde(default)]
    incomplete_details: Option<Incomplete>,
    #[serde(default)]
    error: Option<Failure>,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum Streamed {
    #[serde(rename = "response.output_text.delta")]
    TextDelta { delta: String },
    #[serde(rename = "response.reasoning_summary_text.delta")]
    ReasoningDelta { delta: String },
    #[serde(rename = "response.output_item.done")]
    ItemDone { item: OutputItem },
    #[serde(rename = "response.completed", alias = "response.incomplete")]
    Completed {
        #[serde(default)]
        response: Finished,
    },
    #[serde(rename = "response.failed")]
    Failed {
        #[serde(default)]
        response: Finished,
    },
    #[serde(rename = "error")]
    Error {
        #[serde(default)]
        message: Option<String>,
    },
    #[serde(other)]
    Other,
}

#[derive(Default)]
pub(in crate::engine::provider) struct Parser {
    blocks: Vec<Block>,
    usage: Usage,
    length: bool,
    sources: Vec<Source>,
}

impl Parser {
    fn take_item(&mut self, item: OutputItem) {
        match item {
            OutputItem::Message { content } => {
                for part in content {
                    let OutputPart::OutputText { text, annotations } = part else {
                        continue;
                    };
                    push_text(&mut self.blocks, &text);
                    self.sources
                        .extend(annotations.into_iter().filter_map(
                            |annotation| match annotation {
                                Annotation::UrlCitation { url, title } => {
                                    Some(Source { title, url })
                                }
                                Annotation::Other => None,
                            },
                        ));
                }
            }
            OutputItem::Reasoning {
                summary,
                encrypted_content,
            } => {
                let text = summary
                    .into_iter()
                    .map(|part| part.text)
                    .filter(|text| !text.is_empty())
                    .collect::<Vec<_>>()
                    .join(SUMMARY_JOINER);
                if text.is_empty() && encrypted_content.is_none() {
                    return;
                }
                self.blocks.push(Block::Thinking {
                    text,
                    signature: encrypted_content,
                    protocol: Protocol::Responses,
                });
            }
            OutputItem::FunctionCall {
                call_id,
                name,
                arguments,
            } => self.blocks.push(Block::Call(Call {
                id: call_id,
                name,
                arguments: object_arguments(&arguments),
            })),
            OutputItem::Other => {}
        }
    }

    fn take_finish(&mut self, finished: Finished) {
        if let Some(usage) = finished.usage {
            let cached = usage.input_tokens_details.cached_tokens;
            self.usage = Usage {
                input: usage.input_tokens.saturating_sub(cached),
                output: usage.output_tokens,
                cache_read: cached,
                cache_write: 0,
            };
        }
        self.length = finished
            .incomplete_details
            .and_then(|details| details.reason)
            .is_some_and(|reason| reason == LENGTH_REASON);
    }
}

impl Parse for Parser {
    fn feed(&mut self, data: &str, on_delta: &mut dyn FnMut(Delta)) -> Result<(), GatewayError> {
        match serde_json::from_str::<Streamed>(data)? {
            Streamed::TextDelta { delta } => on_delta(Delta::Text(delta)),
            Streamed::ReasoningDelta { delta } => on_delta(Delta::Thinking(delta)),
            Streamed::ItemDone { item } => self.take_item(item),
            Streamed::Completed { response } => self.take_finish(response),
            Streamed::Failed { response } => {
                let message = response.error.and_then(|failure| failure.message);
                return Err(GatewayError::Failed(message.unwrap_or_default()));
            }
            Streamed::Error { message } => {
                return Err(GatewayError::Failed(
                    message.or_else(|| error_message(data)).unwrap_or_default(),
                ));
            }
            Streamed::Other => {}
        }
        Ok(())
    }

    fn finish(self) -> Reply {
        let calls = self
            .blocks
            .iter()
            .any(|block| matches!(block, Block::Call(_)));
        let stop = if calls {
            Stop::Tools
        } else if self.length {
            Stop::Length
        } else {
            Stop::Finished
        };
        Reply {
            blocks: self.blocks,
            usage: self.usage,
            stop,
            sources: self.sources,
        }
    }
}
