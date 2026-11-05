use super::*;
use serde_json::json;

fn update(value: serde_json::Value) -> SessionUpdate {
    serde_json::from_value(value).unwrap_or_else(|error| unreachable!("a valid update: {error}"))
}

fn tools(events: &[LiveEvent]) -> Vec<&ToolCall> {
    events
        .iter()
        .filter_map(|event| match event {
            LiveEvent::Chat(ChatEvent::Tool(call)) => Some(call),
            _ => None,
        })
        .collect()
}

#[test]
fn a_bare_pending_tool_waits_for_its_details_before_it_shows() {
    let mut translator = Translator::default();
    let pending = translator.translate(update(json!({
        "sessionUpdate": "tool_call", "toolCallId": "c1", "title": "write", "kind": "edit", "status": "pending"
    })));
    assert_eq!(pending, Vec::<LiveEvent>::new());
    let detailed = translator.translate(update(json!({
        "sessionUpdate": "tool_call_update", "toolCallId": "c1", "status": "in_progress",
        "rawInput": {"path": "/w/hello.txt", "content": "hi"}
    })));
    let shown = tools(&detailed);
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].summary, "/w/hello.txt");
    assert_eq!(
        translator.translate(update(
            json!({"sessionUpdate": "tool_call_update", "toolCallId": "c1", "status": "completed"})
        )),
        Vec::<LiveEvent>::new()
    );
}

#[test]
fn a_thought_ends_with_the_next_update_and_an_undetailed_tool_shows_when_the_turn_ends() {
    let mut translator = Translator::default();
    for piece in ["hm", "m"] {
        let thinking = translator.translate(update(json!({
            "sessionUpdate": "agent_thought_chunk", "content": {"type": "text", "text": piece}
        })));
        assert_eq!(thinking, Vec::<LiveEvent>::new());
    }
    let called = translator.translate(update(json!({
        "sessionUpdate": "tool_call", "toolCallId": "c2", "title": "think", "kind": "think", "status": "pending"
    })));
    assert_eq!(
        called,
        [LiveEvent::Chat(ChatEvent::Thinking("hmm".to_owned()))]
    );
    let ended = translator.turn_ended();
    assert_eq!(tools(&ended).len(), 1);
    assert_eq!(translator.turn_ended(), Vec::<LiveEvent>::new());
}

#[test]
fn commands_reach_the_catalog() {
    let mut translator = Translator::default();
    let events = translator.translate(update(json!({
        "sessionUpdate": "available_commands_update",
        "availableCommands": [{"name": "review", "description": "review changes"}]
    })));
    assert!(matches!(
        events.as_slice(),
        [LiveEvent::Ready(catalog)] if catalog.commands[0].name == "review"
    ));
}
