use krad_chat::{parse_line, ChatEvent, ToolCall, ToolUseId};
use krad_domain::{AgentPreset, ResumeId};

fn events(preset: AgentPreset, transcript: &str) -> Vec<ChatEvent> {
    transcript
        .lines()
        .flat_map(|line| parse_line(preset, line))
        .collect()
}

fn model(name: &str) -> ChatEvent {
    ChatEvent::Model(name.to_owned())
}

fn session(id: &str) -> ChatEvent {
    let Ok(id) = id.parse::<ResumeId>() else {
        unreachable!("{id} is a valid resume id");
    };
    ChatEvent::Session(id)
}

fn tool(name: &str, summary: &str) -> ChatEvent {
    ChatEvent::Tool(ToolCall {
        id: None,
        name: name.to_owned(),
        summary: summary.to_owned(),
        edit: None,
    })
}

fn claude_tool(id: &str, name: &str, summary: &str) -> ChatEvent {
    ChatEvent::Tool(ToolCall {
        id: Some(ToolUseId::from(id)),
        name: name.to_owned(),
        summary: summary.to_owned(),
        edit: None,
    })
}

#[test]
fn claude_transcript_yields_the_read_the_answer_and_done() {
    assert_eq!(
        events(
            AgentPreset::ClaudeCode,
            include_str!("fixtures/claude.jsonl")
        ),
        [
            session("s1"),
            model("claude-opus"),
            claude_tool("t1", "Read", "server/auth/refresh.ts"),
            ChatEvent::Text("Both tabs call `rotate()` at once.".to_owned()),
            ChatEvent::Done,
        ]
    );
}

#[test]
fn codex_transcript_folds_commands_and_edits() {
    assert_eq!(
        events(AgentPreset::Codex, include_str!("fixtures/codex.jsonl")),
        [
            tool("Run", "rg rotate server/auth"),
            tool("Edit", "server/auth/refresh.ts"),
            ChatEvent::Text("Share one in-flight promise.".to_owned()),
            ChatEvent::Done,
        ]
    );
}

#[test]
fn opencode_transcript_ends_with_the_process() {
    assert_eq!(
        events(
            AgentPreset::Opencode,
            include_str!("fixtures/opencode.jsonl")
        ),
        [
            tool("read", "/work/portfolio/server/auth/refresh.ts"),
            ChatEvent::Text("The refresh token is spent twice.".to_owned()),
        ]
    );
}

#[test]
fn gemini_transcript_skips_the_echoed_user_message() {
    assert_eq!(
        events(AgentPreset::Gemini, include_str!("fixtures/gemini.jsonl")),
        [
            tool("read_file", "server/auth/refresh.ts"),
            ChatEvent::Text("Two refreshes race.".to_owned()),
            ChatEvent::Done,
        ]
    );
}

#[test]
fn a_recorded_claude_turn_parses_to_its_answer() {
    assert_eq!(
        events(
            AgentPreset::ClaudeCode,
            include_str!("fixtures/claude-recorded.jsonl")
        ),
        [
            ChatEvent::Text("pong".to_owned()),
            ChatEvent::ContextTokens(33_694),
            ChatEvent::ContextWindow(1_000_000),
            ChatEvent::Done
        ],
        "the context the answer took, and the window it fits in"
    );
}

#[test]
fn a_live_claude_conversation_reports_its_session_and_closes_each_answer() {
    let id = "0b7c3a52-9f1e-4d2a-8c6b-1f2e3d4c5b6a";
    assert_eq!(
        events(
            AgentPreset::ClaudeCode,
            include_str!("fixtures/claude-live.jsonl")
        ),
        [
            session(id),
            model("claude-opus"),
            ChatEvent::Text("Two tabs rotate at once.".to_owned()),
            ChatEvent::Done,
            session(id),
            model("claude-opus"),
            claude_tool("t1", "Edit", "server/auth/refresh.ts"),
            ChatEvent::Text("Shared one in-flight promise.".to_owned()),
            ChatEvent::Done,
        ]
    );
}
