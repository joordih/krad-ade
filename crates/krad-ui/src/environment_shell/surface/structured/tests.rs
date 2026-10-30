use super::*;
use krad_domain::AgentPreset;

fn missing() -> ResumeRefused {
    ResumeRefused::MissingConversation(
        "No conversation found with session ID: 4f2c0b7c-a91e".to_owned(),
    )
}

fn id(raw: &str) -> ResumeId {
    raw.parse()
        .unwrap_or_else(|_| unreachable!("{raw} is a valid resume id"))
}

fn resumed() -> StructuredChat {
    StructuredChat::new(
        LiveSession::Resume(id("4f2c0b7c-a91e")),
        AgentPreset::ClaudeCode,
    )
}

fn queued(text: &str) -> Outbox {
    let mut outbox = Outbox::default();
    outbox.push(Prompt::from(text));
    outbox
}

#[test]
fn the_context_reads_as_the_share_of_the_window_in_use() {
    let mut chat = resumed();
    assert_eq!(chat.context_used(), None);
    chat.context_tokens = Some(50_000);
    assert_eq!(chat.context_used(), None, "the window is still unknown");
    chat.context_window = Some(200_000);
    assert_eq!(chat.context_used(), Some(25));
}

#[test]
fn a_toggled_setting_and_a_queued_message_leave_in_one_batch() {
    let mut chat = resumed();
    let mut outbox = queued("Fix the race");
    chat.command(SurfaceCommand::ToggleThinking);
    assert!(chat.has_work(&outbox));
    let batch = chat.take_batch(&mut outbox, Instant::now());
    assert_eq!(batch.commands, [SlashCommand::Thinking(!THINKING_DEFAULT)]);
    assert_eq!(batch.message, Some(Prompt::from("Fix the race")));
    assert!(!chat.has_work(&outbox), "nothing is left");
}

#[test]
fn a_turn_ends_with_the_last_result_owed_not_with_a_setting_s() {
    let mut chat = resumed();
    chat.handed_over(Batch {
        commands: vec![
            SlashCommand::Effort(Effort::High),
            SlashCommand::Thinking(false),
        ],
        message: Some(Prompt::from("Fix the race")),
    });
    assert!(!chat.closes_turn(false), "the effort's result");
    assert!(!chat.closes_turn(false), "the thinking's result");
    assert!(chat.working, "the message is still being answered");
    assert!(chat.closes_turn(false), "the message's result");
    assert!(!chat.working);
}

#[test]
fn a_process_that_ended_owes_nothing_more() {
    let mut chat = resumed();
    chat.handed_over(Batch {
        commands: vec![SlashCommand::Effort(Effort::High)],
        message: Some(Prompt::from("Fix the race")),
    });
    assert!(chat.closes_turn(true));
    assert_eq!(chat.outstanding, 0);
}

#[test]
fn a_reported_session_is_what_the_next_process_resumes() {
    let mut chat = StructuredChat::new(
        LiveSession::Start(id("0b7c3a52-9f1e")),
        AgentPreset::ClaudeCode,
    );
    chat.session_reported(id("4f2c0b7c-a91e"));
    assert_eq!(chat.session, LiveSession::Resume(id("4f2c0b7c-a91e")));
}

#[test]
fn a_resume_of_a_missing_conversation_starts_a_new_one_once() {
    let mut chat = resumed();
    let batch = Batch {
        commands: Vec::new(),
        message: Some(Prompt::from("Fix the race")),
    };
    chat.handed_over(batch.clone());
    let kept = ResumeRefused::TurnNotDropped("Resume rejected".to_owned());
    assert_eq!(chat.retries(&kept), None, "nothing was cut");
    assert_eq!(chat.retries(&missing()), Some(batch));
    assert!(matches!(chat.session, LiveSession::Start(_)));
    assert_eq!(chat.outstanding, 0);
    chat.session = LiveSession::Resume(id("4f2c0b7c-a91e"));
    assert_eq!(chat.retries(&missing()), None, "only once");
}

#[test]
fn a_new_conversation_never_falls_back() {
    let mut chat = StructuredChat::new(
        LiveSession::Start(id("0b7c3a52-9f1e")),
        AgentPreset::ClaudeCode,
    );
    assert_eq!(chat.retries(&missing()), None);
}

#[test]
fn thinking_starts_on_and_each_toggle_is_typed_as_a_setting() {
    let mut chat = resumed();
    assert_eq!(chat.thinking, THINKING_DEFAULT);
    chat.command(SurfaceCommand::ToggleThinking);
    chat.command(SurfaceCommand::ToggleThinking);
    assert_eq!(chat.thinking, THINKING_DEFAULT);
    assert_eq!(
        chat.commands,
        [
            SlashCommand::Thinking(!THINKING_DEFAULT),
            SlashCommand::Thinking(THINKING_DEFAULT)
        ]
    );
}

#[test]
fn a_bus_notice_waits_for_a_free_chat_and_never_jumps_the_queue() {
    let mut chat = resumed();
    assert!(chat.takes_bus_notice(&Outbox::default()));
    assert!(
        !chat.takes_bus_notice(&queued("Fix the race")),
        "the user's message goes first"
    );
    chat.working = true;
    assert!(
        !chat.takes_bus_notice(&Outbox::default()),
        "a turn is running"
    );
}

#[test]
fn a_new_chat_can_move_to_another_harness_until_its_conversation_exists() {
    let mut chat = StructuredChat::new(
        LiveSession::Start(id("0b7c3a52-9f1e")),
        AgentPreset::ClaudeCode,
    );
    chat.effort = Some(Effort::Max);
    assert!(chat.is_fresh());
    chat.switch_harness(AgentPreset::Opencode);
    assert_eq!(chat.harness, AgentPreset::Opencode);
    assert_eq!(chat.effort, None);
    assert!(matches!(chat.session, LiveSession::Start(_)));
    chat.session_reported(id("ses_4f2c0b7ca91e"));
    assert!(!chat.is_fresh());
    assert!(!resumed().is_fresh());
}

#[test]
fn notices_add_up_under_the_chat_and_a_new_harness_reads_mcp_again() {
    let mut problem = None;
    add_notice(&mut problem, "The MCP server files did not start");
    add_notice(&mut problem, "The MCP server docs did not start");
    assert_eq!(
        problem.as_deref(),
        Some("The MCP server files did not start\nThe MCP server docs did not start")
    );
    let mut chat = resumed();
    chat.mcp = McpPrep::Launching;
    chat.switch_harness(AgentPreset::Gateway);
    assert!(matches!(chat.mcp, McpPrep::Unread));
}
