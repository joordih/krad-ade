use super::{follow, named, on, runtime_with, until, TestResult, REFUSES};
use futures::executor::block_on;
use krad_domain::{AcpWanted, AgentPreset, LiveSession, ResumeId, TerminalId};
use krad_protocol::{AgentRef, ChatCommand, ChatOpen, Command, EventKind, Reply};

const CHAT: TerminalId = TerminalId(7);
const RESUME: &str = "4f2c0b7c-a91e";
const CONNECTING: &str = "Connecting to";

#[test]
fn a_resumed_chat_on_an_unreached_host_waits_for_it_and_says_so() -> TestResult {
    let mut data = on("box", Some(REFUSES))?;
    data.nodes = vec![serde_json::from_value(serde_json::json!({
        "id": CHAT.0, "title": "chat", "state": "Idle", "kind": "Chat"
    }))?];
    let runtime = runtime_with(vec![data])?;
    let feed = follow(&runtime)?;
    let chat = AgentRef {
        environment: named("box"),
        terminal: CHAT,
    };
    let start = ChatOpen {
        harness: AgentPreset::ClaudeCode,
        model: None,
        effort: None,
        permission_mode: None,
        mcp_servers: None,
        acp: AcpWanted::default(),
        session: LiveSession::Resume(RESUME.parse::<ResumeId>()?),
        messages: Vec::new(),
        notices: Vec::new(),
    };
    let open = Command::Chat(ChatCommand::Open {
        chat: chat.clone(),
        start: Box::new(start),
    });
    assert_eq!(
        block_on(runtime.send(runtime.next_request(), open))?,
        Reply::Accepted
    );
    until(&feed, |kind| {
        matches!(kind, EventKind::ChatOpened { chat: told, state } | EventKind::ChatState { chat: told, state }
            if *told == chat
                && state.problem.as_deref().is_some_and(|problem| problem.starts_with(CONNECTING)))
    })?;
    let prompt = Command::Chat(ChatCommand::Send {
        chat: chat.clone(),
        prompt: krad_domain::Prompt::from("still there?"),
    });
    assert_eq!(
        block_on(runtime.send(runtime.next_request(), prompt))?,
        Reply::Accepted
    );
    let queued = until(
        &feed,
        |kind| matches!(kind, EventKind::ChatState { chat: told, state } if *told == chat && !state.queued.is_empty()),
    )?;
    let EventKind::ChatState { state, .. } = queued else {
        return Err("the prompt was not queued".into());
    };
    assert!(!state.alive && !state.working, "{state:?}");
    Ok(())
}
