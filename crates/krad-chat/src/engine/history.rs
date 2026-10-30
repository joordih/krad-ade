use krad_domain::{AgentPreset, ChatEntry, ChatEvent, ModelId, RewindPoint};

use super::call::EngineCall;
use super::session::{effective, Record};
use super::tools::Workspace;
use super::wire::{Block, Call, Message, Stop};

fn tool_entries(call: &Call, workspace: &Workspace) -> Vec<ChatEntry> {
    let parsed = EngineCall::parse(call);
    let row = ChatEntry::Event(ChatEvent::Tool(parsed.row(&call.id, workspace)));
    match parsed {
        EngineCall::Checklist(Ok(list)) => vec![row, ChatEntry::Event(ChatEvent::Checklist(list))],
        _ => vec![row],
    }
}

fn assistant_entries(blocks: &[Block], stop: Stop, workspace: &Workspace) -> Vec<ChatEntry> {
    let mut entries: Vec<ChatEntry> = blocks
        .iter()
        .flat_map(|block| match block {
            Block::Text { text } => vec![ChatEntry::Event(ChatEvent::TextDelta(text.clone()))],
            Block::Thinking { text, .. } => {
                vec![ChatEntry::Event(ChatEvent::Thinking(text.clone()))]
            }
            Block::Call(call) => tool_entries(call, workspace),
        })
        .collect();
    match stop.problem() {
        Some(problem) => entries.push(ChatEntry::Event(ChatEvent::Error(problem.to_owned()))),
        None if stop == Stop::Finished => entries.push(ChatEntry::Event(ChatEvent::Done)),
        None => {}
    }
    entries
}

pub(crate) fn transcript(records: Vec<Record>, workspace: &Workspace) -> Vec<ChatEntry> {
    let mut model: Option<ModelId> = None;
    let mut entries = Vec::new();
    let mut prompts = 0;
    for record in effective(records) {
        match record {
            Record::Model { model: chosen, .. } => {
                model = ModelId::parse(&chosen);
                entries.push(ChatEntry::Event(ChatEvent::Model(chosen)));
            }
            Record::Message(Message::User { text, images }) => {
                entries.push(ChatEntry::User {
                    text,
                    preset: AgentPreset::Gateway,
                    model: model.clone(),
                    images,
                });
                entries.push(ChatEntry::Event(ChatEvent::Prompted(RewindPoint::Prompt(
                    prompts,
                ))));
                prompts += 1;
            }
            Record::Message(Message::Assistant { blocks, stop, .. }) => {
                entries.extend(assistant_entries(&blocks, stop, workspace));
            }
            Record::Message(Message::Results { .. })
            | Record::Compacted { .. }
            | Record::Rewound { .. } => {}
        }
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::wire::Usage;
    use serde_json::json;
    use std::path::PathBuf;

    #[test]
    fn a_stored_session_replays_the_user_the_tools_and_the_closed_answers() {
        let workspace = Workspace {
            cwd: PathBuf::from("/work"),
            env: Vec::new(),
        };
        let records = vec![
            Record::Model {
                model: "gpt-5".to_owned(),
                vendor: None,
            },
            Record::Message(Message::user("fix it".to_owned())),
            Record::Message(Message::Assistant {
                blocks: vec![Block::Call(Call {
                    id: "c1".to_owned(),
                    name: "read".to_owned(),
                    arguments: json!({"path": "src/lib.rs"}),
                })],
                usage: Usage::default(),
                stop: Stop::Tools,
            }),
            Record::Message(Message::Results { outcomes: vec![] }),
            Record::Message(Message::Assistant {
                blocks: vec![Block::Text {
                    text: "Done.".to_owned(),
                }],
                usage: Usage::default(),
                stop: Stop::Finished,
            }),
        ];
        let entries = transcript(records, &workspace);
        assert!(matches!(
            &entries[1],
            ChatEntry::User { preset: AgentPreset::Gateway, model: Some(model), .. }
                if model.as_str() == "gpt-5"
        ));
        assert_eq!(
            entries[2],
            ChatEntry::Event(ChatEvent::Prompted(RewindPoint::Prompt(0)))
        );
        assert!(
            matches!(&entries[3], ChatEntry::Event(ChatEvent::Tool(tool)) if tool.id.is_some())
        );
        assert_eq!(
            entries[4..],
            [
                ChatEntry::Event(ChatEvent::TextDelta("Done.".to_owned())),
                ChatEntry::Event(ChatEvent::Done)
            ]
        );
    }

    #[test]
    fn plan_web_and_background_calls_replay_as_labelled_tool_rows() {
        let workspace = Workspace {
            cwd: PathBuf::from("/work"),
            env: Vec::new(),
        };
        let call = |id: &str, name: &str, arguments: serde_json::Value| {
            Block::Call(Call {
                id: id.to_owned(),
                name: name.to_owned(),
                arguments,
            })
        };
        let records = vec![Record::Message(Message::Assistant {
            blocks: vec![
                call("p", "exit_plan", json!({"plan": "## Add caching\n1. ..."})),
                call("f", "web_fetch", json!({"url": "https://docs.rs/serde"})),
                call("s", "web_search", json!({"query": "serde flatten"})),
                call("o", "shell_output", json!({"id": "bg1"})),
                call(
                    "a",
                    "agent",
                    json!({"description": "Map the UI", "prompt": "Find every view"}),
                ),
                call(
                    "c",
                    "checklist",
                    json!({"items": [{"text": "Fix it", "status": "in_progress"}]}),
                ),
                call(
                    "e",
                    "edit",
                    json!({"path": "a.rs", "edits": [{"old_string": "a", "new_string": "b"}]}),
                ),
                call("m", "mcp__docs__search", json!({"q": "serde"})),
            ],
            usage: Usage::default(),
            stop: Stop::Tools,
        })];
        let rows: Vec<(String, String)> = transcript(records, &workspace)
            .into_iter()
            .filter_map(|entry| match entry {
                ChatEntry::Event(ChatEvent::Tool(tool)) => Some((tool.name, tool.summary)),
                _ => None,
            })
            .collect();
        let expected = [
            ("ExitPlanMode", "Add caching"),
            ("WebFetch", "https://docs.rs/serde"),
            ("WebSearch", "serde flatten"),
            ("Bash", "bg1"),
            ("Agent", "Map the UI"),
            ("TodoWrite", "Fix it"),
            ("Edit", "a.rs"),
            ("docs · search", "q: serde"),
        ]
        .map(|(name, summary)| (name.to_owned(), summary.to_owned()));
        assert_eq!(rows, expected);
    }

    #[test]
    fn a_checklist_call_replays_the_checklist_after_its_row() {
        let workspace = Workspace {
            cwd: PathBuf::from("/work"),
            env: Vec::new(),
        };
        let records = vec![Record::Message(Message::Assistant {
            blocks: vec![Block::Call(Call {
                id: "c".to_owned(),
                name: "checklist".to_owned(),
                arguments: json!({"items": [{"text": "Fix it", "status": "done"}]}),
            })],
            usage: Usage::default(),
            stop: Stop::Tools,
        })];
        let entries = transcript(records, &workspace);
        assert!(matches!(
            entries.as_slice(),
            [ChatEntry::Event(ChatEvent::Tool(_)), ChatEntry::Event(ChatEvent::Checklist(list))]
                if list.is_finished()
        ));
    }
}
