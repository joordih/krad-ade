use krad_chat::{
    engine_subagents, new_session_id, read_engine_subagent, read_subagent_meta, AskAnswer, AskKind,
    ChatEvent, LiveEvent, LiveSession,
};
use krad_domain::{ChatEntry, ResumeId};
use serde_json::{json, Value};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use super::support::{
    completions, config, finished, scratch, serve_gateway, start, until, Seen, WAIT,
};

const FIRST_TASK: &str = "SUBTASK-ONE: say what the parser does";
const SECOND_TASK: &str = "SUBTASK-TWO: write the note";
const FIRST_REPORT: &str = "REPORT-ONE";
const SECOND_REPORT: &str = "REPORT-TWO";
const FINAL_ANSWER: &str = "Both done.";
const WRITE_ID: &str = "call_w";
const POLL: Duration = Duration::from_millis(20);

fn routed_gateway(route: fn(&str) -> String) -> io::Result<(String, Seen)> {
    serve_gateway(move |asked| route(&asked.body))
}

fn workspace(name: &str) -> io::Result<PathBuf> {
    let root = scratch(name);
    fs::create_dir_all(root.join("work"))?;
    Ok(root)
}

fn chunk(delta: &Value, finish: &str) -> String {
    let chunk = json!({"choices": [{"index": 0, "delta": delta, "finish_reason": finish}]});
    format!("data: {chunk}\n\ndata: [DONE]\n\n")
}

fn text(answer: &str) -> String {
    chunk(&json!({"content": answer}), "stop")
}

fn calls(calls: &[(&str, &str, Value)]) -> String {
    let calls: Vec<Value> = calls
        .iter()
        .enumerate()
        .map(|(index, (id, name, arguments))| {
            json!({"index": index, "id": id, "type": "function",
                "function": {"name": name, "arguments": arguments.to_string()}})
        })
        .collect();
    chunk(&json!({ "tool_calls": calls }), "tool_calls")
}

fn split_work(body: &str) -> String {
    if body.contains(FIRST_REPORT) && body.contains(SECOND_REPORT) {
        return text(FINAL_ANSWER);
    }
    if body.contains(FIRST_TASK) {
        return text(FIRST_REPORT);
    }
    if body.contains(SECOND_TASK) {
        if body.contains(WRITE_ID) {
            return text(SECOND_REPORT);
        }
        return calls(&[(
            WRITE_ID,
            "write",
            json!({"path": "note.txt", "content": "hi\n"}),
        )]);
    }
    calls(&[
        (
            "agent_1",
            "agent",
            json!({"description": "Explain the parser", "prompt": FIRST_TASK}),
        ),
        (
            "agent_2",
            "agent",
            json!({"description": "Write the note", "prompt": SECOND_TASK}),
        ),
    ])
}

fn write_only(body: &str) -> String {
    if body.contains(SECOND_TASK) {
        return calls(&[(
            WRITE_ID,
            "write",
            json!({"path": "note.txt", "content": "hi\n"}),
        )]);
    }
    calls(&[(
        "agent_2",
        "agent",
        json!({"description": "Write the note", "prompt": SECOND_TASK}),
    )])
}

fn all_finished(sessions: &Path, id: &ResumeId, count: usize) -> bool {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        let files = engine_subagents(sessions, id);
        let done = files
            .iter()
            .filter(|files| read_subagent_meta(&files.meta).is_ok_and(|meta| meta.finished))
            .count();
        if done == count {
            return true;
        }
        thread::sleep(POLL);
    }
    false
}

#[test]
fn two_subagents_run_in_parallel_and_their_reports_go_back_to_the_parent() -> io::Result<()> {
    let root = workspace("parallel")?;
    let (url, seen) = routed_gateway(split_work)?;
    let id = new_session_id();
    let (chat, events) = start(config(&url, &root), &LiveSession::Start(id.clone()));
    until(&events, |event| matches!(event, LiveEvent::Ready(_)));
    chat.send("split the work");
    let asked = until(&events, |event| matches!(event, LiveEvent::Ask(_)));
    let Some(LiveEvent::Ask(ask)) = asked.last() else {
        unreachable!("the second subagent asks before writing: {asked:?}");
    };
    assert!(
        matches!(&ask.kind, AskKind::Permission { tool, .. } if tool == "Write"),
        "{:?}",
        ask.kind
    );
    let agents: Vec<&str> = asked
        .iter()
        .filter_map(|event| match event {
            LiveEvent::Chat(ChatEvent::Tool(tool)) if tool.name == "Agent" => {
                Some(tool.summary.as_str())
            }
            _ => None,
        })
        .collect();
    assert_eq!(agents, ["Explain the parser", "Write the note"]);
    chat.answer(ask, &AskAnswer::Allow);
    let answered = until(&events, finished);
    assert!(matches!(
        answered.last(),
        Some(LiveEvent::Chat(ChatEvent::Done))
    ));
    let said: String = asked
        .iter()
        .chain(&answered)
        .filter_map(|event| match event {
            LiveEvent::Chat(ChatEvent::TextDelta(piece)) => Some(piece.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(said, FINAL_ANSWER, "subagent text stays in their cards");
    assert_eq!(
        fs::read_to_string(root.join("work").join("note.txt"))?,
        "hi\n"
    );
    drop(chat);
    let bodies = completions(&seen);
    let last = bodies.last().map_or("", String::as_str);
    assert!(last.contains(FIRST_REPORT) && last.contains(SECOND_REPORT));
    let sessions = root.join("sessions");
    assert!(all_finished(&sessions, &id, 2));
    let first = engine_subagents(&sessions, &id)
        .into_iter()
        .find(|files| files.record.ends_with("agent_1.jsonl"))
        .ok_or_else(|| io::Error::other("the first subagent kept no record"))?;
    let entries = read_engine_subagent(&first.record, &root.join("work"), 0)?.entries;
    assert!(matches!(entries.first(), Some(ChatEntry::User { text, .. }) if text == FIRST_TASK));
    assert!(entries.iter().any(
        |entry| matches!(entry, ChatEntry::Event(ChatEvent::TextDelta(text)) if text == FIRST_REPORT)
    ));
    Ok(())
}

#[test]
fn interrupting_the_chat_stops_a_subagent_waiting_on_the_user() -> io::Result<()> {
    let root = workspace("interrupt")?;
    let (url, _) = routed_gateway(write_only)?;
    let id = new_session_id();
    let (chat, events) = start(config(&url, &root), &LiveSession::Start(id.clone()));
    until(&events, |event| matches!(event, LiveEvent::Ready(_)));
    chat.send("write a note through a subagent");
    let asked = until(&events, |event| matches!(event, LiveEvent::Ask(_)));
    assert!(matches!(asked.last(), Some(LiveEvent::Ask(_))));
    chat.interrupt();
    let stopped = until(&events, finished);
    assert!(matches!(
        stopped.last(),
        Some(LiveEvent::Chat(ChatEvent::Done))
    ));
    assert!(all_finished(&root.join("sessions"), &id, 1));
    assert!(!root.join("work").join("note.txt").exists());
    Ok(())
}
