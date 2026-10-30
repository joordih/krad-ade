use krad_domain::GatewayAccess;
use std::collections::HashSet;
use std::sync::atomic::AtomicBool;

use super::Ran;
use crate::engine::attachments::Attachments;
use crate::engine::provider::{stream, ProviderError};
use crate::engine::wire::{Block, Message, NativeTool, Protocol, Reply, Request, Source};

const SEARCH_SYSTEM: &str = "You search the web for a coding agent. Search for the query, then answer it concisely with the facts you found, naming the pages they come from. Answer with the findings only.";
const SEARCH_MAX_TOKENS: u64 = 8_192;
const SOURCES_HEADING: &str = "Sources:";
const NO_ANSWER_TEXT: &str = "The search returned no answer.";
const NO_MODEL_TEXT: &str = "Pick a Gateway model before searching the web";
const CANCELLED_TEXT: &str = "[cancelled]";

#[derive(Clone, Debug)]
pub(crate) struct Search {
    pub(crate) access: GatewayAccess,
    pub(crate) model: String,
    pub(crate) protocol: Protocol,
}

fn answer_text(blocks: &[Block]) -> String {
    blocks
        .iter()
        .filter_map(|block| match block {
            Block::Text { text } => Some(text.as_str()),
            Block::Thinking { .. } | Block::Call(_) => None,
        })
        .collect::<String>()
        .trim()
        .to_owned()
}

fn source_line(source: &Source) -> String {
    let title = source.title.trim();
    if title.is_empty() {
        format!("- {}", source.url)
    } else {
        format!("- {title} ({})", source.url)
    }
}

pub(super) fn findings(reply: &Reply) -> String {
    let answer = answer_text(&reply.blocks);
    let answer = if answer.is_empty() {
        NO_ANSWER_TEXT.to_owned()
    } else {
        answer
    };
    let mut seen = HashSet::new();
    let sources: Vec<String> = reply
        .sources
        .iter()
        .filter(|source| seen.insert(source.url.as_str()))
        .map(source_line)
        .collect();
    if sources.is_empty() {
        return answer;
    }
    format!("{answer}\n\n{SOURCES_HEADING}\n{}", sources.join("\n"))
}

pub(super) fn search(search: Option<&Search>, query: &str, cancel: &AtomicBool) -> Ran {
    let Some(search) = search else {
        return Ran::failed(NO_MODEL_TEXT.to_owned());
    };
    let messages = [Message::user(query.to_owned())];
    let request = Request {
        model: &search.model,
        system: SEARCH_SYSTEM,
        messages: &messages,
        images: Attachments::none(),
        tools: &[],
        native: Some(NativeTool::WebSearch),
        effort: None,
        max_tokens: SEARCH_MAX_TOKENS,
    };
    match stream(
        &search.access,
        search.protocol,
        &request,
        cancel,
        &mut |_| {},
    ) {
        Ok(reply) => Ran::done(findings(&reply)),
        Err(ProviderError::Cancelled) => Ran::failed(CANCELLED_TEXT.to_owned()),
        Err(error) => Ran::failed(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn findings_list_each_source_once_after_the_answer() {
        let source = |title: &str, url: &str| Source {
            title: title.to_owned(),
            url: url.to_owned(),
        };
        let reply = Reply {
            blocks: vec![Block::Text {
                text: " Rust 1.90 is out. ".to_owned(),
            }],
            sources: vec![
                source("Rust Blog", "https://blog.rust-lang.org/a"),
                source("", "https://example.org"),
                source("Again", "https://blog.rust-lang.org/a"),
            ],
            ..Reply::default()
        };
        assert_eq!(
            findings(&reply),
            "Rust 1.90 is out.\n\nSources:\n- Rust Blog (https://blog.rust-lang.org/a)\n- https://example.org"
        );
        assert_eq!(findings(&Reply::default()), NO_ANSWER_TEXT);
    }
}
