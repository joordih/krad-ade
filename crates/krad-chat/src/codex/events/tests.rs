use super::*;
use krad_domain::LineChange;
use serde_json::json;

fn chat(events: Vec<LiveEvent>) -> Vec<ChatEvent> {
    events
        .into_iter()
        .filter_map(|event| match event {
            LiveEvent::Chat(event) => Some(event),
            LiveEvent::Ask(_)
            | LiveEvent::Ready(_)
            | LiveEvent::Usage(_)
            | LiveEvent::Notice(_)
            | LiveEvent::Rewound(_)
            | LiveEvent::Refused(_) => None,
        })
        .collect()
}

#[test]
fn a_turn_becomes_its_tools_text_usage_and_end() {
    let mut translator = Translator::default();
    let notices = [
        (
            TURN_STARTED,
            json!({"threadId": "t", "turn": {"id": "turn-1", "items": [], "status": "inProgress", "error": null}}),
        ),
        (
            ITEM_STARTED,
            json!({"threadId": "t", "turnId": "turn-1", "item": {"type": "commandExecution", "id": "exec-1",
            "command": "/bin/zsh -lc 'git status'", "status": "inProgress",
            "commandActions": [{"type": "unknown", "command": "git status"}]}}),
        ),
        (
            ITEM_COMPLETED,
            json!({"threadId": "t", "turnId": "turn-1", "item": {"type": "commandExecution", "id": "exec-1",
            "command": "/bin/zsh -lc 'git status'", "status": "completed", "commandActions": []}}),
        ),
        (
            ITEM_COMPLETED,
            json!({"threadId": "t", "turnId": "turn-1", "item": {"type": "fileChange", "id": "patch-1",
            "status": "completed", "changes": [{"path": "src/a.rs", "kind": {"type": "update", "move_path": null},
            "diff": "@@ -3,2 +3,2 @@\n fn a() {}\n-old\n+new\n"}]}}),
        ),
        (
            ITEM_COMPLETED,
            json!({"threadId": "t", "turnId": "turn-1", "item": {"type": "agentMessage", "id": "m1",
            "text": "Clean tree.", "phase": "final_answer"}}),
        ),
        (
            TOKEN_USAGE,
            json!({"threadId": "t", "turnId": "turn-1", "tokenUsage": {
            "total": {"totalTokens": 1, "inputTokens": 1, "cachedInputTokens": 0, "outputTokens": 0, "reasoningOutputTokens": 0},
            "last": {"totalTokens": 14316, "inputTokens": 14263, "cachedInputTokens": 12288, "outputTokens": 53, "reasoningOutputTokens": 0},
            "modelContextWindow": 475_000}}),
        ),
    ];
    let events: Vec<ChatEvent> = notices
        .iter()
        .flat_map(|(method, params)| chat(translator.translate(method, params)))
        .collect();
    assert_eq!(translator.turn.as_deref(), Some("turn-1"));
    let ChatEvent::Tool(edit) = &events[2] else {
        unreachable!("the file change is a tool");
    };
    let lines: Vec<(LineChange, Option<u32>, Option<u32>)> =
        edit.edit.as_ref().map_or_default(|edit| {
            edit.lines
                .iter()
                .map(|line| (line.change, line.old_line, line.new_line))
                .collect()
        });
    assert_eq!(
        lines,
        [
            (LineChange::Kept, Some(3), Some(3)),
            (LineChange::Removed, Some(4), None),
            (LineChange::Added, None, Some(4)),
        ]
    );
    assert_eq!(
        events,
        [
            ChatEvent::Prompted(RewindPoint::Turn(RewindId::from("turn-1"))),
            ChatEvent::Tool(call("exec-1", BASH_TOOL, "git status".to_owned(), None)),
            events[2].clone(),
            ChatEvent::Text("Clean tree.".to_owned()),
            ChatEvent::ContextTokens(14_263),
            ChatEvent::ContextWindow(475_000),
        ]
    );
    let done = translator.translate(
        TURN_COMPLETED,
        &json!({"threadId": "t", "turn": {"id": "turn-1", "items": [], "status": "failed",
            "error": {"message": "usage limit reached"}}}),
    );
    assert_eq!(
        chat(done),
        [ChatEvent::Error("usage limit reached".to_owned())]
    );
    assert_eq!(translator.turn, None);
}

#[test]
fn rate_limits_fill_the_five_hour_and_weekly_rings() {
    let mut translator = Translator::default();
    let events = translator.translate(
        RATE_LIMITS,
        &json!({"rateLimits": {"limitId": "codex",
            "primary": {"usedPercent": 12, "windowDurationMins": 300, "resetsAt": 1},
            "secondary": {"usedPercent": 40, "windowDurationMins": 10_080, "resetsAt": 2}}}),
    );
    assert_eq!(
        events,
        [LiveEvent::Usage(PlanUsage {
            five_hour: Some(12),
            seven_day: Some(40),
        })]
    );
}

#[test]
fn an_updated_plan_reaches_the_chat_as_its_checklist() {
    let events = chat(Translator::default().translate(
        PLAN_UPDATED,
        &json!({"threadId": "t", "turnId": "turn-1",
            "plan": [{"step": "Fix the bug", "status": "inProgress"}]}),
    ));
    assert!(matches!(
        events.as_slice(),
        [ChatEvent::Checklist(list)] if list.items.len() == 1
    ));
}

#[test]
fn a_codex_subagent_call_is_told_apart_as_a_subagent() {
    let events = chat(Translator::default().translate(
        ITEM_STARTED,
        &json!({"threadId": "t", "turnId": "turn-1", "item": {"type": "collabAgentToolCall",
            "id": "agent-1", "prompt": "Map the UI"}}),
    ));
    assert!(
        matches!(
            events.as_slice(),
            [ChatEvent::Tool(call)] if call.kind == krad_domain::ToolKind::Subagent
                && call.summary == "Map the UI"
        ),
        "{events:?}"
    );
}
