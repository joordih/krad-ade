use super::*;
use krad_domain::LineChange;

fn event(line: &str) -> ServerEvent {
    serde_json::from_str(line).unwrap_or_else(|_| unreachable!("the fixture is valid"))
}

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
fn a_turn_becomes_text_tools_usage_and_its_end() {
    let windows = HashMap::from([("anthropic/claude-sonnet-5-5".to_owned(), 200_000)]);
    let mut translator = Translator::new(windows, None);
    let lines = [
        r#"{"type":"session.step.started","data":{"sessionID":"ses_a","assistantMessageID":"msg_1","model":{"providerID":"anthropic","id":"claude-sonnet-5-5"}}}"#,
        r#"{"type":"session.reasoning.ended","data":{"sessionID":"ses_a","assistantMessageID":"msg_1","ordinal":0,"text":"  look first  "}}"#,
        r#"{"type":"session.tool.input.started","data":{"sessionID":"ses_a","assistantMessageID":"msg_1","id":"call_1","name":"shell"}}"#,
        r#"{"type":"session.tool.called","data":{"sessionID":"ses_a","assistantMessageID":"msg_1","id":"call_1","input":{"command":"git status"},"executed":false}}"#,
        r#"{"type":"session.text.ended","data":{"sessionID":"ses_a","assistantMessageID":"msg_1","ordinal":1,"text":"Clean tree."}}"#,
        r#"{"type":"session.step.ended","data":{"sessionID":"ses_a","assistantMessageID":"msg_1","finish":"stop","cost":0.01,"tokens":{"input":1200,"output":85,"reasoning":0,"cache":{"read":9000,"write":10}}}}"#,
        r#"{"type":"session.execution.succeeded","data":{"sessionID":"ses_a"}}"#,
    ];
    let events: Vec<ChatEvent> = lines
        .iter()
        .flat_map(|line| chat(translator.translate(&event(line), false)))
        .collect();
    assert_eq!(
        events,
        [
            ChatEvent::Model("anthropic/claude-sonnet-5-5".to_owned()),
            ChatEvent::ContextWindow(200_000),
            ChatEvent::Thinking("look first".to_owned()),
            ChatEvent::Tool(ToolCall {
                id: Some(ToolUseId::from("call_1")),
                name: "Bash".to_owned(),
                kind: krad_domain::ToolKind::Shell,
                summary: "git status".to_owned(),
                edit: None,
            }),
            ChatEvent::Text("Clean tree.".to_owned()),
            ChatEvent::ContextTokens(10_210),
            ChatEvent::Done,
        ]
    );
}

#[test]
fn a_failed_execution_carries_the_reason_opencode_gave() {
    let mut translator = Translator::default();
    let failed = event(
        r#"{"type":"session.execution.failed","data":{"sessionID":"ses_a","error":{"type":"provider.free-limit","message":"Free usage limit reached.","status":429}}}"#,
    );
    assert_eq!(
        chat(translator.translate(&failed, false)),
        [ChatEvent::Error("Free usage limit reached.".to_owned())]
    );
}

#[test]
fn a_subagent_only_reaches_the_chat_through_its_questions() {
    let mut translator = Translator::default();
    let text =
        event(r#"{"type":"session.text.ended","data":{"sessionID":"ses_child","text":"inner"}}"#);
    let asked = event(
        r#"{"type":"permission.asked","data":{"id":"per_1","sessionID":"ses_child","action":"shell","resources":["rm -rf build"]}}"#,
    );
    assert_eq!(translator.translate(&text, true), []);
    assert!(matches!(
        translator.translate(&asked, true).as_slice(),
        [LiveEvent::Ask(_)]
    ));
}

#[test]
fn an_edit_shows_its_diff() {
    let edit = opencode_edit(
        EDIT_TOOL,
        &serde_json::json!({"path":"src/a.rs","oldString":"one\n","newString":"two\n"}),
    )
    .unwrap_or_else(|| unreachable!("the fixture has it"));
    assert_eq!(edit.path, "src/a.rs");
    let changes: Vec<LineChange> = edit.lines.iter().map(|line| line.change).collect();
    assert_eq!(changes, [LineChange::Removed, LineChange::Added]);
}

#[test]
fn switching_to_the_plan_agent_reports_plan_mode() {
    let mut translator = Translator::new(HashMap::new(), Some(PermissionMode::AcceptEdits));
    let plan =
        event(r#"{"type":"session.agent.selected","data":{"sessionID":"ses_a","agent":"plan"}}"#);
    let build =
        event(r#"{"type":"session.agent.selected","data":{"sessionID":"ses_a","agent":"build"}}"#);
    assert_eq!(
        chat(translator.translate(&plan, false)),
        [ChatEvent::PermissionMode(PermissionMode::Plan)]
    );
    assert_eq!(
        chat(translator.translate(&build, false)),
        [ChatEvent::PermissionMode(PermissionMode::Manual)]
    );
}

#[test]
fn a_todowrite_call_sends_the_checklist_after_its_tool_row() {
    let mut translator = Translator::default();
    let lines = [
        r#"{"type":"session.tool.input.started","data":{"sessionID":"ses_a","id":"call_t","name":"todowrite"}}"#,
        r#"{"type":"session.tool.called","data":{"sessionID":"ses_a","id":"call_t","input":{"todos":[{"content":"Draw it","status":"in_progress","priority":"high","id":"1"}]}}}"#,
    ];
    let events: Vec<ChatEvent> = lines
        .iter()
        .flat_map(|line| chat(translator.translate(&event(line), false)))
        .collect();
    assert!(matches!(
        events.as_slice(),
        [ChatEvent::Tool(tool), ChatEvent::Checklist(list)]
            if tool.name == "TodoWrite" && list.items.len() == 1
    ));
}
