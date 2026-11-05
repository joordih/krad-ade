use krad_domain::{ChatEvent, ToolKind};
use serde_json::json;
use std::path::PathBuf;
use std::sync::PoisonError;

use super::fake::{start, Started, SESSION};
use super::{fresh, opened, prompted};
use crate::control::LiveEvent;

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("krad-acp-chat-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap_or_default();
        Self(dir)
    }

    fn file(&self, name: &str, content: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, content).unwrap_or_default();
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap_or_default();
    }
}

fn chat_until(started: &Started, mut wanted: impl FnMut(&ChatEvent) -> bool) -> Vec<ChatEvent> {
    started
        .until(|event| matches!(event, LiveEvent::Chat(event) if wanted(event)))
        .into_iter()
        .filter_map(|event| match event {
            LiveEvent::Chat(event) => Some(event),
            _ => None,
        })
        .collect()
}

#[test]
fn a_read_through_the_chat_prefers_the_unsaved_tab() {
    let scratch = Scratch::new("read");
    let path = scratch.file("a.txt", "saved\n");
    let mut started = start(&fresh());
    started
        .buffers
        .unsaved
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(path.clone(), "unsaved\n".to_owned());
    opened(&mut started, &json!({"sessionId": SESSION}));
    started.agent.ask(
        "read-1",
        "fs/read_text_file",
        &json!({"sessionId": SESSION, "path": path}),
    );
    let answer = started.agent.next();
    assert_eq!(answer["id"], "read-1");
    assert_eq!(answer["result"]["content"], "unsaved\n");
}

#[test]
fn a_write_during_the_only_open_tool_lands_on_disk_and_becomes_its_edit() {
    let scratch = Scratch::new("write-tool");
    let path = scratch.file("a.txt", "old\n");
    let mut started = start(&fresh());
    opened(&mut started, &json!({"sessionId": SESSION}));
    let prompt = prompted(&mut started, "edit it");
    started
        .agent
        .update(&json!({"sessionUpdate": "tool_call", "toolCallId": "t1",
        "title": "edit", "kind": "edit", "status": "in_progress", "locations": [{"path": path}]}));
    started.agent.ask(
        "write-1",
        "fs/write_text_file",
        &json!({"sessionId": SESSION, "path": path, "content": "new\n"}),
    );
    let answer = started.agent.next();
    assert_eq!(answer["id"], "write-1");
    assert_eq!(answer["result"], json!({}));
    let events = chat_until(&started, |event| matches!(event, ChatEvent::Edited { .. }));
    assert!(
        matches!(events.last(), Some(ChatEvent::Edited { tool, edit })
        if tool.as_str() == "t1" && edit.counts() == (1, 1))
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap_or_default(), "new\n");
    let written = started
        .buffers
        .written
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    assert_eq!(written, [(path, "new\n".to_owned())]);
    started
        .agent
        .reply(&prompt, &json!({"stopReason": "end_turn"}));
    chat_until(&started, |event| *event == ChatEvent::Done);
}

#[test]
fn a_write_with_no_open_tool_shows_as_a_write_of_its_own() {
    let scratch = Scratch::new("write-alone");
    let path = scratch.0.join("fresh.txt");
    let mut started = start(&fresh());
    opened(&mut started, &json!({"sessionId": SESSION}));
    let _prompt = prompted(&mut started, "write it");
    started.agent.ask(
        "write-1",
        "fs/write_text_file",
        &json!({"sessionId": SESSION, "path": path, "content": "one\ntwo\n"}),
    );
    let events = chat_until(&started, |event| matches!(event, ChatEvent::Tool(_)));
    assert!(matches!(events.last(), Some(ChatEvent::Tool(call))
        if call.kind == ToolKind::Edit
            && call.edit.as_ref().is_some_and(|edit| edit.counts() == (2, 0))));
}

#[cfg(unix)]
#[test]
fn a_terminal_in_a_tool_call_streams_its_output_until_it_exits() {
    let scratch = Scratch::new("terminal");
    let mut started = start(&fresh());
    opened(&mut started, &json!({"sessionId": SESSION}));
    let prompt = prompted(&mut started, "run it");
    started.agent.ask(
        "create-1",
        "terminal/create",
        &json!({"sessionId": SESSION, "command": "printf", "args": ["hello"], "cwd": scratch.0}),
    );
    let created = started.agent.next();
    let terminal = created["result"]["terminalId"].clone();
    started
        .agent
        .update(&json!({"sessionUpdate": "tool_call", "toolCallId": "t1",
        "title": "printf hello", "kind": "execute", "status": "in_progress",
        "content": [{"type": "terminal", "terminalId": terminal}]}));
    let events = chat_until(
        &started,
        |event| matches!(event, ChatEvent::ToolOutput { output, .. } if output.exit_code.is_some()),
    );
    assert!(
        matches!(events.last(), Some(ChatEvent::ToolOutput { tool, output })
        if tool.as_str() == "t1" && output.text == "hello" && output.exit_code == Some(0))
    );
    started
        .agent
        .reply(&prompt, &json!({"stopReason": "end_turn"}));
    chat_until(&started, |event| *event == ChatEvent::Done);
}
