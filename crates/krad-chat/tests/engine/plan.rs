use krad_chat::{new_session_id, AskAnswer, AskKind, ChatEvent, LiveEvent, LiveSession};
use krad_domain::PermissionMode;
use std::fs;
use std::io;

use super::support::{
    call_stream, completions, config, fake_gateway, finished, scratch, start, until, ANSWER,
    WRITE_CALL,
};

#[test]
fn an_approved_plan_switches_the_mode_and_the_turn_goes_on_to_implement_it() -> io::Result<()> {
    let root = scratch("plan");
    fs::create_dir_all(root.join("work"))?;
    let plan = call_stream(
        "call_plan",
        "exit_plan",
        &serde_json::json!({"plan": "# Write the note\n1. write note.txt"}),
    );
    let (url, seen) = fake_gateway(vec![plan, WRITE_CALL.to_owned(), ANSWER.to_owned()])?;
    let mut planning = config(&url, &root);
    planning.permission_mode = Some(PermissionMode::Plan);
    let (chat, events) = start(planning, &LiveSession::Start(new_session_id()));
    until(&events, |event| matches!(event, LiveEvent::Ready(_)));
    chat.send("plan a note");
    let asked = until(&events, |event| matches!(event, LiveEvent::Ask(_)));
    let Some(LiveEvent::Ask(ask)) = asked.last() else {
        unreachable!("exit_plan in Plan mode asks for approval: {asked:?}");
    };
    let AskKind::Plan { plan } = &ask.kind else {
        unreachable!("exit_plan asks with the plan: {:?}", ask.kind);
    };
    assert!(plan.starts_with("# Write the note"));
    assert!(asked.iter().any(|event| matches!(
        event,
        LiveEvent::Chat(ChatEvent::Tool(tool)) if tool.name == "ExitPlanMode"
    )));
    chat.answer(
        ask,
        &AskAnswer::ApprovePlan {
            mode: PermissionMode::AcceptEdits,
        },
    );
    let answered = until(&events, finished);
    assert!(answered.iter().any(|event| matches!(
        event,
        LiveEvent::Chat(ChatEvent::PermissionMode(PermissionMode::AcceptEdits))
    )));
    assert!(
        !answered
            .iter()
            .any(|event| matches!(event, LiveEvent::Ask(_))),
        "the write needs no approval once edits are accepted"
    );
    assert!(matches!(
        answered.last(),
        Some(LiveEvent::Chat(ChatEvent::Done))
    ));
    assert_eq!(
        fs::read_to_string(root.join("work").join("note.txt"))?,
        "hi\n"
    );
    drop(chat);
    let bodies = completions(&seen);
    assert_eq!(bodies.len(), 3);
    assert!(bodies[1].contains("approved the plan"), "{}", bodies[1]);
    Ok(())
}

#[test]
fn exit_plan_outside_plan_mode_just_tells_the_model_to_proceed() -> io::Result<()> {
    let root = scratch("no-plan");
    fs::create_dir_all(root.join("work"))?;
    let plan = call_stream(
        "call_plan",
        "exit_plan",
        &serde_json::json!({"plan": "# Steps"}),
    );
    let (url, seen) = fake_gateway(vec![plan, ANSWER.to_owned()])?;
    let (chat, events) = start(config(&url, &root), &LiveSession::Start(new_session_id()));
    until(&events, |event| matches!(event, LiveEvent::Ready(_)));
    chat.send("do it");
    let answered = until(&events, finished);
    assert!(!answered
        .iter()
        .any(|event| matches!(event, LiveEvent::Ask(_))));
    drop(chat);
    let bodies = completions(&seen);
    assert!(bodies
        .last()
        .is_some_and(|body| body.contains("not in Plan mode")));
    Ok(())
}

#[test]
fn a_rejected_plan_keeps_plan_mode_and_sends_the_feedback() -> io::Result<()> {
    let root = scratch("plan-rejected");
    fs::create_dir_all(root.join("work"))?;
    let plan = call_stream(
        "call_plan",
        "exit_plan",
        &serde_json::json!({"plan": "# Big rewrite"}),
    );
    let (url, seen) = fake_gateway(vec![plan, ANSWER.to_owned()])?;
    let mut planning = config(&url, &root);
    planning.permission_mode = Some(PermissionMode::Plan);
    let (chat, events) = start(planning, &LiveSession::Start(new_session_id()));
    until(&events, |event| matches!(event, LiveEvent::Ready(_)));
    chat.send("plan it");
    let asked = until(&events, |event| matches!(event, LiveEvent::Ask(_)));
    let Some(LiveEvent::Ask(ask)) = asked.last() else {
        unreachable!("exit_plan in Plan mode asks for approval: {asked:?}");
    };
    chat.answer(
        ask,
        &AskAnswer::Deny {
            message: "smaller steps".to_owned(),
        },
    );
    let answered = until(&events, finished);
    assert!(!answered
        .iter()
        .any(|event| matches!(event, LiveEvent::Chat(ChatEvent::PermissionMode(_)))));
    drop(chat);
    let bodies = completions(&seen);
    let last = bodies.last().map_or("", String::as_str);
    assert!(
        last.contains("rejected the plan") && last.contains("smaller steps"),
        "{last}"
    );
    Ok(())
}
