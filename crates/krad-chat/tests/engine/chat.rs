use krad_chat::{
    new_session_id, read_engine_transcript, AskAnswer, AskKind, ChatEvent, LiveEvent, LiveSession,
    PickedAnswer,
};
use krad_domain::ChatEntry;
use std::fs;
use std::io;
use std::sync::PoisonError;

use super::support::{
    call_stream, completions, config, fake_gateway, finished, scratch, start, streams, until,
    Asked, ANSWER, KEY, MODEL, QUESTION_CALL, WRITE_CALL,
};

#[test]
fn a_gateway_chat_asks_before_writing_then_answers_and_resumes() -> io::Result<()> {
    let root = scratch("write");
    fs::create_dir_all(root.join("work"))?;
    let (url, seen) = fake_gateway(streams(&[WRITE_CALL, ANSWER]))?;
    let id = new_session_id();
    let (chat, events) = start(config(&url, &root), &LiveSession::Start(id.clone()));
    let opened = until(&events, |event| matches!(event, LiveEvent::Ready(_)));
    assert!(opened.iter().any(
        |event| matches!(event, LiveEvent::Chat(ChatEvent::Session(started)) if *started == id)
    ));
    chat.send("write a note");
    let asked = until(&events, |event| matches!(event, LiveEvent::Ask(_)));
    let Some(LiveEvent::Ask(ask)) = asked.last() else {
        unreachable!("a write in manual mode asks first: {asked:?}");
    };
    assert!(!root.join("work").join("note.txt").exists());
    chat.answer(ask, &AskAnswer::Allow);
    let answered = until(&events, |event| {
        matches!(
            event,
            LiveEvent::Chat(ChatEvent::Done | ChatEvent::Error(_))
        )
    });
    assert_eq!(
        fs::read_to_string(root.join("work").join("note.txt"))?,
        "hi\n"
    );
    assert!(answered
        .iter()
        .any(|event| matches!(event, LiveEvent::Chat(ChatEvent::Edited { .. }))));
    let text: String = answered
        .iter()
        .filter_map(|event| match event {
            LiveEvent::Chat(ChatEvent::TextDelta(piece)) => Some(piece.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(text, "Wrote it.");
    assert!(matches!(
        answered.last(),
        Some(LiveEvent::Chat(ChatEvent::Done))
    ));
    drop(chat);
    let asked = seen.lock().unwrap_or_else(PoisonError::into_inner);
    let turns: Vec<&Asked> = asked
        .iter()
        .filter(|asked| asked.path == "/v1/chat/completions")
        .collect();
    assert_eq!(turns.len(), 2);
    assert!(turns
        .iter()
        .all(|turn| turn.authorization.as_deref() == Some(&format!("Bearer {KEY}"))));
    assert!(turns[1].body.contains("call_1"), "the result goes back");
    let entries = read_engine_transcript(&root.join("sessions"), &root.join("work"), &id)?;
    assert_eq!(
        entries.first(),
        Some(&ChatEntry::Event(ChatEvent::Model(MODEL.to_owned())))
    );
    assert!(matches!(
        entries.get(1),
        Some(ChatEntry::User { text, model: Some(model), .. })
            if text == "write a note" && model.as_str() == MODEL
    ));
    assert!(entries.iter().any(
        |entry| matches!(entry, ChatEntry::Event(ChatEvent::TextDelta(text)) if text == "Wrote it.")
    ));
    Ok(())
}

#[test]
fn a_denied_write_goes_back_to_the_model_as_refused() -> io::Result<()> {
    let root = scratch("deny");
    fs::create_dir_all(root.join("work"))?;
    let (url, seen) = fake_gateway(streams(&[WRITE_CALL, ANSWER]))?;
    let (chat, events) = start(config(&url, &root), &LiveSession::Start(new_session_id()));
    until(&events, |event| matches!(event, LiveEvent::Ready(_)));
    chat.send("write a note");
    let asked = until(&events, |event| matches!(event, LiveEvent::Ask(_)));
    let Some(LiveEvent::Ask(ask)) = asked.last() else {
        unreachable!("a write in manual mode asks first");
    };
    chat.answer(
        ask,
        &AskAnswer::Deny {
            message: "not now".to_owned(),
        },
    );
    until(&events, |event| {
        matches!(
            event,
            LiveEvent::Chat(ChatEvent::Done | ChatEvent::Error(_))
        )
    });
    assert!(!root.join("work").join("note.txt").exists());
    drop(chat);
    let asked = seen.lock().unwrap_or_else(PoisonError::into_inner);
    let last = asked
        .iter()
        .rfind(|asked| asked.path == "/v1/chat/completions")
        .map_or("", |asked| asked.body.as_str());
    assert!(last.contains("not now"), "{last}");
    Ok(())
}

#[test]
fn the_model_can_ask_the_user_and_gets_the_picked_answers_back() -> io::Result<()> {
    let root = scratch("questions");
    fs::create_dir_all(root.join("work"))?;
    let (url, seen) = fake_gateway(streams(&[QUESTION_CALL, ANSWER]))?;
    let (chat, events) = start(config(&url, &root), &LiveSession::Start(new_session_id()));
    until(&events, |event| matches!(event, LiveEvent::Ready(_)));
    chat.send("sort a list");
    let asked = until(&events, |event| matches!(event, LiveEvent::Ask(_)));
    let Some(LiveEvent::Ask(ask)) = asked.last() else {
        unreachable!("the model asks the user: {asked:?}");
    };
    let AskKind::Questions(questions) = &ask.kind else {
        unreachable!("an ask_user call is a question form: {:?}", ask.kind);
    };
    assert_eq!(questions[0].text, "Which sort?");
    assert!(!ask.can_allow_always(), "questions are never remembered");
    chat.answer(
        ask,
        &AskAnswer::Answers(vec![PickedAnswer {
            question: "Which sort?".to_owned(),
            labels: vec!["Merge sort".to_owned()],
        }]),
    );
    let answered = until(&events, |event| {
        matches!(
            event,
            LiveEvent::Chat(ChatEvent::Done | ChatEvent::Error(_))
        )
    });
    assert!(matches!(
        answered.last(),
        Some(LiveEvent::Chat(ChatEvent::Done))
    ));
    drop(chat);
    let asked = seen.lock().unwrap_or_else(PoisonError::into_inner);
    let last = asked
        .iter()
        .rfind(|asked| asked.path == "/v1/chat/completions")
        .map_or("", |asked| asked.body.as_str());
    assert!(last.contains("Which sort?: Merge sort"), "{last}");
    Ok(())
}

#[test]
fn a_checklist_call_reaches_the_chat_without_asking_and_is_confirmed() -> io::Result<()> {
    let root = scratch("checklist");
    fs::create_dir_all(root.join("work"))?;
    let checklist = call_stream(
        "call_c",
        "checklist",
        &serde_json::json!({"items": [
            {"text": "Read the parser", "status": "done"},
            {"text": "Fix the bug", "status": "in_progress"}
        ]}),
    );
    let (url, seen) = fake_gateway(vec![checklist, ANSWER.to_owned()])?;
    let id = new_session_id();
    let (chat, events) = start(config(&url, &root), &LiveSession::Start(id.clone()));
    until(&events, |event| matches!(event, LiveEvent::Ready(_)));
    chat.send("fix the parser");
    let turn = until(&events, finished);
    assert!(!turn.iter().any(|event| matches!(event, LiveEvent::Ask(_))));
    assert!(turn.iter().any(|event| matches!(
        event,
        LiveEvent::Chat(ChatEvent::Checklist(list)) if list.done() == 1 && list.items.len() == 2
    )));
    let bodies = completions(&seen);
    assert!(bodies
        .last()
        .is_some_and(|body| body.contains("Checklist updated: 1 of 2 done.")));
    drop(chat);
    let entries = read_engine_transcript(&root.join("sessions"), &root.join("work"), &id)?;
    assert!(entries
        .iter()
        .any(|entry| matches!(entry, ChatEntry::Event(ChatEvent::Checklist(_)))));
    Ok(())
}
