use serde_json::{json, Value};

use super::ENVIRONMENT;

pub(super) fn file_commands() -> Vec<(&'static str, Value)> {
    vec![
        (
            "file_read",
            json!({ "area": "file", "environment": ENVIRONMENT, "ask": { "verb": "read", "path": "/home/dev/app/src/main.rs", "from": 0, "limit": 2_097_152, "over": "refuse" } }),
        ),
        (
            "file_write",
            json!({ "area": "file", "environment": ENVIRONMENT, "ask": { "verb": "write", "path": "/home/dev/app/src/main.rs", "text": "fn main() {}\n" } }),
        ),
        (
            "file_upload",
            json!({ "area": "file", "environment": ENVIRONMENT, "ask": { "verb": "upload", "dir": "/home/dev/app/assets", "sources": ["/Users/dev/Desktop/logo.png"], "existing": "skip" } }),
        ),
        (
            "file_watch",
            json!({ "area": "file", "environment": ENVIRONMENT, "ask": { "verb": "watch", "dir": "/home/dev/app/src" } }),
        ),
        (
            "file_search",
            json!({ "area": "file", "environment": ENVIRONMENT, "ask": { "verb": "search", "root": "/home/dev/app", "query": { "pattern": "todo", "syntax": "literal", "case": "insensitive", "word": "anywhere", "include": ["*.rs"], "exclude": [] }, "limit": 2000 } }),
        ),
    ]
}

pub(super) fn file_events() -> Vec<(&'static str, Value)> {
    vec![
        (
            "answered_file_text",
            json!({ "feed": "event", "seq": 53, "kind": { "type": "answered", "request": 18, "answer": { "kind": "file_text", "text": "fn main() {}\n", "len": 13, "truncated": false, "modified": { "secs_since_epoch": 1_700_000_000, "nanos_since_epoch": 0 } } } }),
        ),
        (
            "answered_unreadable",
            json!({ "feed": "event", "seq": 54, "kind": { "type": "answered", "request": 19, "answer": { "kind": "unreadable", "unreadable": { "reason": "too_large", "len": 9_000_000, "limit": 2_097_152 } } } }),
        ),
        (
            "folder_changed",
            json!({ "feed": "event", "seq": 56, "kind": { "type": "folder_changed", "environment": ENVIRONMENT, "dir": "/home/dev/app/src", "entries": [{ "name": "main.rs", "path": "/home/dev/app/src/main.rs", "kind": "file" }] } }),
        ),
        (
            "upload_progress",
            json!({ "feed": "event", "seq": 55, "kind": { "type": "upload_progress", "upload": 20, "sent": 4096, "total": 8192 } }),
        ),
    ]
}
