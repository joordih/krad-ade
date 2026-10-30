use std::collections::HashSet;
use std::sync::atomic::AtomicBool;

use super::side::{Asked, SideRequest};
use super::Ran;
use crate::engine::wire::{answer_text, NativeTool, Reply, Source};
use crate::gateway::{GatewayError, Vendor};

const SEARCH_SYSTEM: &str = "You search the web for a coding agent. Search for the query, then answer it concisely with the facts you found, naming the pages they come from. Answer with the findings only.";
const SEARCH_MAX_TOKENS: u64 = 8_192;
const SOURCES_HEADING: &str = "Sources:";
const NO_ANSWER_TEXT: &str = "The search returned no answer.";
const NO_MODEL_TEXT: &str = "Pick a Gateway model before searching the web";
const CANCELLED_TEXT: &str = "[cancelled]";
const NO_NATIVE_SEARCH_LEAD: &str = "Web search is not available for ";
const NO_NATIVE_SEARCH_TAIL: &str = ": the Gateway offers native search only for Anthropic, OpenAI and Google models. Use web_fetch with an address you know instead.";

const fn native_search(vendor: Vendor) -> Option<NativeTool> {
    match vendor {
        Vendor::Anthropic | Vendor::OpenAi | Vendor::Google => Some(NativeTool::WebSearch),
        Vendor::Other => None,
    }
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
    let answer = answer_text(&reply.blocks).trim().to_owned();
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

pub(super) fn search(side: Option<&SideRequest>, query: &str, cancel: &AtomicBool) -> Ran {
    let Some(side) = side else {
        return Ran::failed(NO_MODEL_TEXT.to_owned());
    };
    let Some(native) = native_search(side.vendor) else {
        return Ran::failed(format!(
            "{NO_NATIVE_SEARCH_LEAD}{}{NO_NATIVE_SEARCH_TAIL}",
            side.model
        ));
    };
    let asked = Asked {
        system: SEARCH_SYSTEM,
        text: query.to_owned(),
        native: Some(native),
        max_tokens: SEARCH_MAX_TOKENS,
    };
    match side.ask(asked, cancel) {
        Ok(reply) => Ran::done(findings(&reply)),
        Err(GatewayError::Cancelled) => Ran::failed(CANCELLED_TEXT.to_owned()),
        Err(error) => Ran::failed(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::wire::Block;

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
