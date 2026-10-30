use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

use super::super::sse::Event;
use super::super::{error_message, object_arguments, push_text, Parse, ProviderError};
use crate::engine::wire::{Block, Call, Delta, Protocol, Reply, Stop, Usage};

const TOOLS_FINISH: &str = "tool_calls";
const LENGTH_FINISH: &str = "length";

#[derive(Default, Deserialize)]
struct StreamedFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Deserialize)]
struct StreamedCall {
    #[serde(default)]
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: StreamedFunction,
}

#[derive(Default, Deserialize)]
struct ChoiceDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    reasoning: Option<String>,
    #[serde(default)]
    tool_calls: Vec<StreamedCall>,
}

#[derive(Deserialize)]
struct Choice {
    #[serde(default)]
    delta: ChoiceDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Default, Deserialize)]
struct PromptDetails {
    #[serde(default)]
    cached_tokens: u64,
}

#[derive(Deserialize)]
struct ChunkUsage {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
    #[serde(default)]
    prompt_tokens_details: Option<PromptDetails>,
}

#[derive(Deserialize)]
struct Chunk {
    #[serde(default)]
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<ChunkUsage>,
    #[serde(default)]
    error: Option<Value>,
}

#[derive(Default)]
struct PartialCall {
    id: String,
    name: String,
    arguments: String,
}

#[derive(Default)]
pub(in crate::engine::provider) struct Parser {
    thinking: String,
    text: String,
    calls: BTreeMap<usize, PartialCall>,
    usage: Usage,
    stop: Stop,
}

impl Parser {
    fn take_delta(&mut self, delta: ChoiceDelta, on_delta: &mut dyn FnMut(Delta)) {
        if let Some(thought) = delta.reasoning_content.or(delta.reasoning) {
            if !thought.is_empty() {
                self.thinking.push_str(&thought);
                on_delta(Delta::Thinking(thought));
            }
        }
        if let Some(text) = delta.content.filter(|text| !text.is_empty()) {
            self.text.push_str(&text);
            on_delta(Delta::Text(text));
        }
        for call in delta.tool_calls {
            let partial = self.calls.entry(call.index).or_default();
            if let Some(id) = call.id.filter(|id| !id.is_empty()) {
                partial.id = id;
            }
            if let Some(name) = call.function.name {
                partial.name.push_str(&name);
            }
            if let Some(arguments) = call.function.arguments {
                partial.arguments.push_str(&arguments);
            }
        }
    }
}

fn stop_of(reason: &str) -> Stop {
    match reason {
        TOOLS_FINISH => Stop::Tools,
        LENGTH_FINISH => Stop::Length,
        _ => Stop::Finished,
    }
}

impl Parse for Parser {
    fn feed(
        &mut self,
        event: &Event,
        on_delta: &mut dyn FnMut(Delta),
    ) -> Result<(), ProviderError> {
        let chunk: Chunk = serde_json::from_str(&event.data)?;
        if chunk.error.is_some() {
            return Err(ProviderError::Failed(
                error_message(&event.data).unwrap_or_default(),
            ));
        }
        if let Some(usage) = chunk.usage {
            let cached = usage
                .prompt_tokens_details
                .map_or(0, |details| details.cached_tokens);
            self.usage = Usage {
                input: usage.prompt_tokens.saturating_sub(cached),
                output: usage.completion_tokens,
                cache_read: cached,
                cache_write: 0,
            };
        }
        for choice in chunk.choices {
            self.take_delta(choice.delta, on_delta);
            if let Some(reason) = choice.finish_reason {
                self.stop = stop_of(&reason);
            }
        }
        Ok(())
    }

    fn finish(self) -> Reply {
        let mut blocks = Vec::new();
        if !self.thinking.is_empty() {
            blocks.push(Block::Thinking {
                text: self.thinking,
                signature: None,
                protocol: Protocol::Completions,
            });
        }
        push_text(&mut blocks, &self.text);
        let calls: Vec<Block> = self
            .calls
            .into_values()
            .filter(|call| !call.name.is_empty())
            .map(|call| {
                Block::Call(Call {
                    id: call.id,
                    name: call.name,
                    arguments: object_arguments(&call.arguments),
                })
            })
            .collect();
        let stop = if calls.is_empty() {
            self.stop
        } else {
            Stop::Tools
        };
        blocks.extend(calls);
        Reply {
            blocks,
            usage: self.usage,
            stop,
            sources: Vec::new(),
        }
    }
}
