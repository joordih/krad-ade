use super::worker::{lead_environment, main_environment, start, until, Folder};
use super::{answered, call_from, not_found, TestResult, ENVIRONMENT, LEAD, MISSING};
use futures::executor::block_on;
use krad_agents::{Request, Response};
use krad_chat::new_session_id;
use krad_domain::{
    AcpWanted, AgentPreset, EnvironmentEdit, EnvironmentName, LiveSession, SessionEdit,
    SessionLead, Speaker, TerminalId,
};
use krad_protocol::{AgentRef, ChatCommand, ChatOpen, Command, EventKind, Reply};

const PEER: TerminalId = TerminalId(2);
const ANSWER: &str = "Two tabs rotate at once.";

fn peer_open() -> ChatOpen {
    ChatOpen {
        harness: AgentPreset::ClaudeCode,
        model: None,
        effort: None,
        permission_mode: None,
        mcp_servers: None,
        acp: AcpWanted::default(),
        session: LiveSession::Start(new_session_id()),
        messages: Vec::new(),
        notices: Vec::new(),
    }
}

#[test]
fn a_lead_s_message_to_a_chat_it_does_not_lead_is_queued_by_the_runtime() -> TestResult {
    let folder =
        Folder(std::env::temp_dir().join(format!("krad-peer-{}", new_session_id().as_str())));
    std::fs::create_dir_all(&folder.0)?;
    let mut data = lead_environment(&folder.0)?;
    let group = data
        .groups
        .first()
        .map(|group| group.id.0)
        .ok_or("no group")?;
    data.nodes.push(serde_json::from_value(serde_json::json!({
        "id": PEER.0, "title": "peer", "state": "Idle", "kind": "Chat", "group": group
    }))?);
    data.next_id = PEER.0 + 1;
    let session = data
        .active_session
        .ok_or("sessions mode opened no session")?;
    let runtime = start(data)?;
    let mut events = runtime.subscribe();
    let environment = EnvironmentName::new(ENVIRONMENT.to_owned());
    let made_lead: Command = (
        environment.clone(),
        EnvironmentEdit::from(SessionEdit::MakeLead {
            session,
            lead: SessionLead {
                terminal: Some(LEAD),
                ..SessionLead::new(AgentPreset::ClaudeCode, None)
            },
        }),
    )
        .into();
    assert_eq!(
        block_on(runtime.send(runtime.next_request(), made_lead))?,
        Reply::Accepted
    );
    let peer = AgentRef {
        environment,
        terminal: PEER,
    };
    let open = Command::Chat(ChatCommand::Open {
        chat: peer.clone(),
        start: Box::new(peer_open()),
    });
    assert_eq!(
        block_on(runtime.send(runtime.next_request(), open))?,
        Reply::Accepted
    );
    let sent = answered(call_from(
        &runtime,
        LEAD,
        Request::Send {
            to: PEER.0,
            body: "Why do the tabs race?".to_owned(),
        },
    )?)?;
    assert_eq!(sent, Response::Submitted { terminal: PEER.0 });
    until(&mut events, |kind| {
        matches!(kind, EventKind::ChatMessageAdded { chat, message, .. }
            | EventKind::ChatMessageChanged { chat, message, .. }
            if *chat == peer && message.speaker == Speaker::Assistant && message.text == ANSWER)
    })?;
    let missing = answered(call_from(
        &runtime,
        LEAD,
        Request::Read {
            target: MISSING.0,
            lines: 5,
        },
    )?)?;
    assert!(not_found(&missing), "{missing:?}");
    assert!(main_environment(&runtime)?
        .nodes
        .iter()
        .any(|node| node.id == PEER));
    Ok(())
}
