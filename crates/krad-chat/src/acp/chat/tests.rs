mod fake;
mod live;

use krad_domain::{
    AcpAuthMethodId, AcpModeId, AcpPermissionKind, AcpPermissionOptionId, AcpSignIn, ChatEvent,
    Checklist, ChecklistStatus, Prompt, ResumeId, ToolKind,
};
use serde_json::json;

use crate::control::{AcpEvent, AskAnswer, AskKind, LiveEvent};
use crate::live::{AcpOrder, LiveHarness, LiveSession, Order};
use crate::refused::ResumeRefused;
use fake::{order, start, Started, CWD, SESSION};

fn resume_id(text: &str) -> ResumeId {
    ResumeId::try_from(text.to_owned()).unwrap_or_else(|_| unreachable!("{text} is a resume id"))
}

fn fresh() -> LiveSession {
    LiveSession::Start(resume_id("app-chosen"))
}

fn is_ready(event: &LiveEvent) -> bool {
    matches!(event, LiveEvent::Ready(_))
}

fn opened(started: &mut Started, session: &serde_json::Value) -> Vec<LiveEvent> {
    started.agent.initialized(&json!({}), &json!([]));
    let new = started.agent.expect("session/new");
    started.agent.reply(&new, session);
    started.until(is_ready)
}

fn prompted(started: &mut Started, text: &str) -> serde_json::Value {
    order(started, Order::Send(Prompt::from(text)));
    started.agent.expect("session/prompt")
}

#[test]
fn a_new_chat_advertises_the_client_side_and_reports_what_the_agent_offers() {
    let mut started = start(&fresh());
    let initialize = started.agent.initialized(&json!({}), &json!([]));
    assert_eq!(
        initialize["params"]["clientCapabilities"],
        json!({"fs": {"readTextFile": true, "writeTextFile": true}, "terminal": true, "auth": {"terminal": true}})
    );
    let new = started.agent.expect("session/new");
    assert_eq!(new["params"], json!({"cwd": CWD, "mcpServers": []}));
    started.agent.reply(
        &new,
        &json!({
            "sessionId": SESSION,
            "modes": {"currentModeId": "ask", "availableModes": [{"id": "ask", "name": "Ask"}, {"id": "code", "name": "Code"}]},
            "models": {"currentModelId": "sonnet", "availableModels": [{"modelId": "sonnet", "name": "Sonnet"}]}
        }),
    );
    let events = started.until(is_ready);
    assert!(
        matches!(&events[0], LiveEvent::Chat(ChatEvent::Session(id)) if id.as_str() == SESSION)
    );
    assert!(events.iter().any(|event| matches!(
        event,
        LiveEvent::Acp(AcpEvent::Modes(modes)) if modes.available.len() == 2 && modes.current.0 == "ask"
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        LiveEvent::Chat(ChatEvent::Model(name)) if name == "Sonnet"
    )));
}

#[test]
fn a_prompt_streams_thoughts_text_tools_edits_and_the_plan_until_done() {
    let mut started = start(&fresh());
    opened(&mut started, &json!({"sessionId": SESSION}));
    let prompt = prompted(&mut started, "fix it");
    assert_eq!(
        prompt["params"],
        json!({"sessionId": SESSION, "prompt": [{"type": "text", "text": "fix it"}]})
    );
    let agent = &mut started.agent;
    for piece in ["Let me ", "look."] {
        agent.update(&json!({"sessionUpdate": "agent_thought_chunk", "content": {"type": "text", "text": piece}}));
    }
    for piece in ["On ", "it."] {
        agent.update(&json!({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": piece}}));
    }
    agent.update(&json!({"sessionUpdate": "tool_call", "toolCallId": "t1", "title": "write", "kind": "edit", "status": "pending"}));
    agent.update(
        &json!({"sessionUpdate": "tool_call_update", "toolCallId": "t1", "status": "in_progress",
        "locations": [{"path": "/work/a.txt"}]}),
    );
    agent.update(
        &json!({"sessionUpdate": "tool_call_update", "toolCallId": "t1", "status": "completed",
        "content": [{"type": "diff", "path": "/work/a.txt", "oldText": "a\n", "newText": "b\n"}]}),
    );
    agent.update(&json!({"sessionUpdate": "plan", "entries": [
        {"content": "Fix", "priority": "high", "status": "completed"},
        {"content": "Test", "priority": "low", "status": "pending"}]}));
    agent.update(&json!({"sessionUpdate": "usage_update", "used": 10, "size": 100}));
    agent.reply(&prompt, &json!({"stopReason": "end_turn"}));
    let chat: Vec<ChatEvent> = started
        .until(|event| matches!(event, LiveEvent::Chat(ChatEvent::Done)))
        .into_iter()
        .filter_map(|event| match event {
            LiveEvent::Chat(event) => Some(event),
            _ => None,
        })
        .collect();
    assert!(matches!(&chat[0], ChatEvent::Thinking(thought) if thought == "Let me look."));
    assert_eq!(chat[1], ChatEvent::TextDelta("On ".to_owned()));
    assert_eq!(chat[2], ChatEvent::TextDelta("it.".to_owned()));
    assert!(matches!(&chat[3], ChatEvent::Tool(call)
        if call.kind == ToolKind::Edit && call.summary == "/work/a.txt" && call.edit.is_none()));
    assert!(matches!(&chat[4], ChatEvent::Edited { tool, edit }
        if tool.as_str() == "t1" && edit.counts() == (1, 1)));
    assert!(matches!(&chat[5], ChatEvent::Checklist(Checklist { items })
        if items[0].status == ChecklistStatus::Done && items.len() == 2));
    assert_eq!(chat[6], ChatEvent::ContextTokens(10));
    assert_eq!(chat[7], ChatEvent::ContextWindow(100));
    assert_eq!(chat[8], ChatEvent::Done);
}

#[test]
fn a_permission_ask_carries_the_agents_options_and_answers_with_the_chosen_one() {
    let mut started = start(&fresh());
    opened(&mut started, &json!({"sessionId": SESSION}));
    let prompt = prompted(&mut started, "run it");
    started.agent.ask(
        "perm-1",
        "session/request_permission",
        &json!({
            "sessionId": SESSION,
            "toolCall": {"toolCallId": "t9", "title": "cargo test", "kind": "execute", "rawInput": {"command": "cargo test"}},
            "options": [
                {"optionId": "yes", "name": "Allow", "kind": "allow_once"},
                {"optionId": "always", "name": "Always", "kind": "allow_always"},
                {"optionId": "no", "name": "Reject", "kind": "reject_once"}
            ]
        }),
    );
    let ask = match started.event() {
        LiveEvent::Ask(ask) => ask,
        other => unreachable!("expected an ask, got {other:?}"),
    };
    let AskKind::AcpPermission { tool, options } = &ask.kind else {
        unreachable!("expected an ACP permission, got {:?}", ask.kind);
    };
    assert_eq!(tool.summary, "cargo test");
    assert_eq!(tool.kind, ToolKind::Shell);
    assert_eq!(options[1].kind, AcpPermissionKind::AllowAlways);
    assert!(ask.can_allow_always());
    order(
        &started,
        Order::Answer(
            ask.clone(),
            AskAnswer::Chosen(AcpPermissionOptionId("always".to_owned())),
        ),
    );
    let answer = started.agent.next();
    assert_eq!(answer["id"], "perm-1");
    assert_eq!(
        answer["result"],
        json!({"outcome": {"outcome": "selected", "optionId": "always"}})
    );
    started
        .agent
        .reply(&prompt, &json!({"stopReason": "end_turn"}));
    started.until(|event| matches!(event, LiveEvent::Chat(ChatEvent::Done)));
}

#[test]
fn the_client_side_methods_are_refused_until_they_are_served() {
    let mut started = start(&fresh());
    opened(&mut started, &json!({"sessionId": SESSION}));
    started.agent.ask(
        "read-1",
        "fs/read_text_file",
        &json!({"sessionId": SESSION, "path": "/work/a.txt"}),
    );
    let answer = started.agent.next();
    assert_eq!(answer["id"], "read-1");
    assert_eq!(answer["error"]["code"], -32601);
}

#[test]
fn a_mode_is_switched_through_set_mode_or_through_its_config_option() {
    let mut started = start(&fresh());
    opened(
        &mut started,
        &json!({"sessionId": SESSION,
            "modes": {"currentModeId": "ask", "availableModes": [{"id": "ask", "name": "Ask"}, {"id": "code", "name": "Code"}]}}),
    );
    order(
        &started,
        Order::Acp(AcpOrder::Mode(AcpModeId("code".to_owned()))),
    );
    let set = started.agent.expect("session/set_mode");
    assert_eq!(
        set["params"],
        json!({"sessionId": SESSION, "modeId": "code"})
    );
    started.agent.reply(&set, &json!({}));
    let events = started.until(|event| matches!(event, LiveEvent::Acp(AcpEvent::Modes(_))));
    assert!(
        matches!(events.last(), Some(LiveEvent::Acp(AcpEvent::Modes(modes))) if modes.current.0 == "code")
    );

    let mut configured = start(&fresh());
    let mode_option = |current: &str| {
        json!([{"id": "mode", "name": "Session Mode", "category": "mode", "type": "select", "currentValue": current,
            "options": [{"value": "build", "name": "Build"}, {"value": "plan", "name": "Plan"}]}])
    };
    opened(
        &mut configured,
        &json!({"sessionId": SESSION, "configOptions": mode_option("build")}),
    );
    order(
        &configured,
        Order::Acp(AcpOrder::Mode(AcpModeId("plan".to_owned()))),
    );
    let set = configured.agent.expect("session/set_config_option");
    assert_eq!(
        set["params"],
        json!({"sessionId": SESSION, "configId": "mode", "value": "plan"})
    );
    configured
        .agent
        .reply(&set, &json!({"configOptions": mode_option("plan")}));
    let events = configured.until(|event| matches!(event, LiveEvent::Acp(AcpEvent::Modes(_))));
    assert!(
        matches!(events.last(), Some(LiveEvent::Acp(AcpEvent::Modes(modes))) if modes.current.0 == "plan")
    );
}

#[test]
fn an_agent_that_needs_a_sign_in_offers_its_methods_and_opens_the_session_after_it() {
    let mut started = start(&fresh());
    started.agent.initialized(&json!({}), &json!([{"id": "login", "name": "Log in"}, {"type": "terminal", "id": "cli", "name": "In a terminal", "args": ["login"]}]),
    );
    let new = started.agent.expect("session/new");
    started.agent.fail(&new, -32000, "Authentication required");
    let methods = match started.event() {
        LiveEvent::Acp(AcpEvent::SignInNeeded(methods)) => methods,
        other => unreachable!("expected a sign-in, got {other:?}"),
    };
    assert_eq!(methods[0].sign_in, AcpSignIn::ByAgent);
    assert!(
        matches!(&methods[1].sign_in, AcpSignIn::InTerminal { args, .. } if args == &["login"])
    );
    order(&started, Order::Send(Prompt::from("hello")));
    order(
        &started,
        Order::Acp(AcpOrder::SignIn(AcpAuthMethodId("login".to_owned()))),
    );
    let authenticate = started.agent.expect("authenticate");
    assert_eq!(authenticate["params"], json!({"methodId": "login"}));
    started.agent.reply(&authenticate, &json!({}));
    let new = started.agent.expect("session/new");
    started.agent.reply(&new, &json!({"sessionId": SESSION}));
    let events = started.until(is_ready);
    assert!(
        matches!(&events[0], LiveEvent::Chat(ChatEvent::Session(id)) if id.as_str() == SESSION)
    );
    let prompt = started.agent.expect("session/prompt");
    assert_eq!(prompt["params"]["prompt"][0]["text"], "hello");
}

#[test]
fn a_resumed_chat_loads_its_session_without_repeating_the_replayed_history() {
    let mut started = start(&LiveSession::Resume(resume_id(SESSION)));
    started
        .agent
        .initialized(&json!({"loadSession": true}), &json!([]));
    let load = started.agent.expect("session/load");
    assert_eq!(
        load["params"],
        json!({"sessionId": SESSION, "cwd": CWD, "mcpServers": []})
    );
    started.agent.update(&json!({"sessionUpdate": "user_message_chunk", "content": {"type": "text", "text": "old question"}}));
    started.agent.update(&json!({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "old answer"}}));
    started.agent.reply(&load, &json!({}));
    let events = started.until(is_ready);
    assert!(
        matches!(&events[0], LiveEvent::Chat(ChatEvent::Session(id)) if id.as_str() == SESSION)
    );
    let prompt = prompted(&mut started, "go on");
    started.agent.update(&json!({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "new answer"}}));
    started
        .agent
        .reply(&prompt, &json!({"stopReason": "end_turn"}));
    let texts: Vec<String> = started
        .until(|event| matches!(event, LiveEvent::Chat(ChatEvent::Done)))
        .into_iter()
        .filter_map(|event| match event {
            LiveEvent::Chat(ChatEvent::TextDelta(text)) => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["new answer"]);
}

#[test]
fn a_resumed_chat_with_an_agent_that_cannot_load_is_refused_so_it_starts_fresh() {
    let mut started = start(&LiveSession::Resume(resume_id(SESSION)));
    started
        .agent
        .initialized(&json!({"loadSession": false}), &json!([]));
    assert!(matches!(
        started.event(),
        LiveEvent::Refused(ResumeRefused::MissingConversation(text)) if text.starts_with("Fake (ACP) cannot reopen")
    ));
    assert!(started.ended());
    assert!(started.agent.sent_nothing());
}

#[test]
fn a_load_the_agent_refuses_is_refused_too() {
    let mut started = start(&LiveSession::Resume(resume_id(SESSION)));
    started
        .agent
        .initialized(&json!({"loadSession": true}), &json!([]));
    let load = started.agent.expect("session/load");
    started.agent.fail(&load, -32002, "Session not found");
    assert!(matches!(
        started.event(),
        LiveEvent::Refused(ResumeRefused::MissingConversation(text)) if text.contains("Session not found")
    ));
}

#[test]
fn an_interrupt_cancels_the_turn_and_its_open_asks() {
    let mut started = start(&fresh());
    opened(&mut started, &json!({"sessionId": SESSION}));
    let prompt = prompted(&mut started, "long work");
    started.agent.ask(
        "perm-2",
        "session/request_permission",
        &json!({"sessionId": SESSION, "toolCall": {"toolCallId": "t2"},
            "options": [{"optionId": "yes", "name": "Allow", "kind": "allow_once"}]}),
    );
    assert!(matches!(started.event(), LiveEvent::Ask(_)));
    order(&started, Order::Interrupt);
    let cancel = started.agent.expect("session/cancel");
    assert_eq!(cancel["params"], json!({"sessionId": SESSION}));
    assert!(cancel.get("id").is_none());
    let answer = started.agent.next();
    assert_eq!(answer["id"], "perm-2");
    assert_eq!(
        answer["result"],
        json!({"outcome": {"outcome": "cancelled"}})
    );
    started
        .agent
        .reply(&prompt, &json!({"stopReason": "cancelled"}));
    started.until(|event| matches!(event, LiveEvent::Chat(ChatEvent::Done)));
}

#[test]
fn a_rewind_is_refused_and_the_chat_ends_when_the_agent_goes_away() {
    let mut started = start(&fresh());
    opened(&mut started, &json!({"sessionId": SESSION}));
    order(
        &started,
        Order::Rewind(crate::rewind::RewindRequest {
            point: krad_domain::RewindPoint::Prompt(0),
            scope: krad_domain::RewindScope::Conversation,
            home: None,
        }),
    );
    assert!(matches!(
        started.event(),
        LiveEvent::Rewound(Err(crate::rewind::RewindError::Unsupported))
    ));
    let Started {
        chat,
        events,
        agent,
    } = started;
    drop(agent);
    let stopped = events
        .recv_timeout(fake::WAIT)
        .unwrap_or_else(|_| unreachable!("the chat reports the agent stopped"));
    assert!(
        matches!(stopped, LiveEvent::Chat(ChatEvent::Error(text)) if text.starts_with("the agent stopped"))
    );
    chat.stop();
}
