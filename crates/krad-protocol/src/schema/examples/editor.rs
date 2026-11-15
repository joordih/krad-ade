use serde_json::{json, Value};

use super::ENVIRONMENT;

pub(super) fn editor_commands() -> Vec<(&'static str, Value)> {
    vec![
        (
            "editor_open",
            json!({ "area": "editor", "verb": "open", "environment": ENVIRONMENT, "path": "/home/dev/app/src/main.rs" }),
        ),
        (
            "editor_edit",
            json!({ "area": "editor", "verb": "edit", "buffer": 1, "based_on": 7, "edit": { "start": 12, "end": 12, "text": "\n    run();" } }),
        ),
        (
            "editor_save",
            json!({ "area": "editor", "verb": "save", "buffer": 1 }),
        ),
        (
            "language_hover",
            json!({ "area": "language", "buffer": 1, "ask": { "verb": "hover", "at": { "line": 0, "character": 4 } } }),
        ),
        (
            "language_rename",
            json!({ "area": "language", "buffer": 1, "ask": { "verb": "rename", "at": { "line": 3, "character": 7 }, "new_name": "start" } }),
        ),
        (
            "language_install_server",
            json!({ "area": "language", "buffer": 1, "ask": { "verb": "install_server" } }),
        ),
        (
            "editor_close",
            json!({ "area": "editor", "verb": "close", "buffer": 1 }),
        ),
    ]
}

pub(super) fn editor_events() -> Vec<(&'static str, Value)> {
    vec![
        (
            "buffer_opened",
            json!({ "feed": "event", "seq": 60, "kind": { "type": "buffer_opened", "buffer": 1, "environment": ENVIRONMENT, "path": "/home/dev/app/src/main.rs", "version": 0, "saved": 0, "text": "fn main() {}\n" } }),
        ),
        (
            "buffer_edited",
            json!({ "feed": "event", "seq": 61, "kind": { "type": "buffer_edited", "buffer": 1, "version": 8, "edit": { "start": 12, "end": 12, "text": "\n    run();" }, "author": 21 } }),
        ),
        (
            "buffer_saved",
            json!({ "feed": "event", "seq": 62, "kind": { "type": "buffer_saved", "buffer": 1, "version": 8 } }),
        ),
        (
            "diagnostics",
            json!({ "feed": "event", "seq": 65, "kind": { "type": "diagnostics", "environment": ENVIRONMENT, "path": "/home/dev/app/src/main.rs", "diagnostics": [{ "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 2 } }, "severity": "error", "message": "expected `;`", "source": "rustc" }] } }),
        ),
        (
            "language_server",
            json!({ "feed": "event", "seq": 66, "kind": { "type": "language_server", "environment": ENVIRONMENT, "root": "/home/dev/app", "package": "rust-analyzer", "state": { "state": "running" } } }),
        ),
        (
            "answered_hover",
            json!({ "feed": "event", "seq": 67, "kind": { "type": "answered", "request": 23, "answer": { "kind": "language", "answer": { "kind": "hover", "markdown": "`fn main()`" } } } }),
        ),
        (
            "buffer_closed",
            json!({ "feed": "event", "seq": 63, "kind": { "type": "buffer_closed", "buffer": 1 } }),
        ),
        (
            "buffer_not_opened",
            json!({ "feed": "event", "seq": 64, "kind": { "type": "buffer_not_opened", "request": 22, "environment": ENVIRONMENT, "path": "/home/dev/app/logo.png", "message": "the file is not UTF-8 text" } }),
        ),
    ]
}
