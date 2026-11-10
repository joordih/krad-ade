use serde::Deserialize;
use serde_json::{json, Value};

use super::{Bus, OUTSIDE_KRAD};
use crate::protocol::{MemoryEntryInfo, MemoryHitInfo, Request, Response, MAX_MEMORY_HITS};

const MEMORY_SEARCH: &str = "memory_search";
const MEMORY_READ: &str = "memory_read";
const SEARCH_DESCRIPTION: &str = "Search the memory this chat may read: the keepsakes and chat digests of its own Kraddie, of the Kraddies it reads or the user mentioned, and of ordinary chats. Returns memory ids with short excerpts.";
const READ_DESCRIPTION: &str =
    "Read one memory in full by the id memory_search returned, such as chats/digest/3.";
const NO_HITS: &str = "No memory matches";
const HIT_GAP: &str = "  ";
const SEARCH_NEEDS_QUERY: &str = "memory_search needs a query";
const READ_NEEDS_ID: &str = "memory_read needs an id";

#[derive(Default, Deserialize)]
struct SearchArguments {
    #[serde(default)]
    query: String,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Default, Deserialize)]
struct ReadArguments {
    #[serde(default)]
    id: String,
}

pub(super) fn memory_schemas() -> [Value; 2] {
    [
        json!({
            "name": MEMORY_SEARCH,
            "description": SEARCH_DESCRIPTION,
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Words to look for"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": MAX_MEMORY_HITS},
                },
                "required": ["query"],
            },
            "annotations": {"readOnlyHint": true, "destructiveHint": false},
        }),
        json!({
            "name": MEMORY_READ,
            "description": READ_DESCRIPTION,
            "inputSchema": {
                "type": "object",
                "properties": {"id": {"type": "string", "description": "A memory id"}},
                "required": ["id"],
            },
            "annotations": {"readOnlyHint": true, "destructiveHint": false},
        }),
    ]
}

fn arguments<T: Default + for<'de> Deserialize<'de>>(arguments: Value) -> Result<T, String> {
    if arguments.is_null() {
        return Ok(T::default());
    }
    serde_json::from_value(arguments).map_err(|error| error.to_string())
}

pub(super) fn memory_request(name: &str, given: Value) -> Option<Result<Request, String>> {
    match name {
        MEMORY_SEARCH => Some(arguments::<SearchArguments>(given).and_then(|search| {
            if search.query.trim().is_empty() {
                return Err(SEARCH_NEEDS_QUERY.to_owned());
            }
            Ok(Request::MemorySearch {
                query: search.query,
                limit: search.limit,
            })
        })),
        MEMORY_READ => Some(arguments::<ReadArguments>(given).and_then(|read| {
            if read.id.trim().is_empty() {
                return Err(READ_NEEDS_ID.to_owned());
            }
            Ok(Request::MemoryRead { id: read.id })
        })),
        _ => None,
    }
}

fn render_hit(hit: &MemoryHitInfo) -> String {
    let title = hit
        .title
        .as_ref()
        .map_or_default(|title| format!("{title}{HIT_GAP}"));
    format!("{}{HIT_GAP}{title}{}", hit.id, hit.excerpt)
}

fn render_entry(entry: &MemoryEntryInfo) -> String {
    match &entry.title {
        Some(title) => format!("{} ({title})\n{}", entry.id, entry.text),
        None => format!("{}\n{}", entry.id, entry.text),
    }
}

pub(super) fn run_memory(bus: &impl Bus, request: Request) -> (String, bool) {
    match bus.ask(request) {
        Ok(Response::MemoryHits(hits)) if hits.is_empty() => (NO_HITS.to_owned(), false),
        Ok(Response::MemoryHits(hits)) => (
            hits.iter().map(render_hit).collect::<Vec<_>>().join("\n"),
            false,
        ),
        Ok(Response::MemoryEntry(entry)) => (render_entry(&entry), false),
        Ok(Response::Error(error)) => (error.message, true),
        Err(message) => (message, true),
        Ok(_) => (OUTSIDE_KRAD.to_owned(), true),
    }
}
