use super::*;
use krad_domain::AgentPreset;

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
    outbox.push(krad_domain::Prompt::from(text));
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
fn a_working_chat_is_freed_once_the_runtime_ends_its_turn() {
    let mut chat = resumed();
    chat.working = true;
    assert!(!chat.is_free());
    chat.ends_turn();
    assert!(chat.is_free());
}
