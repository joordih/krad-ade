use krad_chat::{
    new_session_id, read_engine_transcript, AskAnswer, ChatEvent, LiveEvent, LiveSession,
    RewindPoint, RewindRequest, RewindScope, Rewound,
};
use krad_domain::ChatEntry;
use std::fs;
use std::io;

use super::support::{
    completions, config, fake_gateway, finished, scratch, start, streams, until, ANSWER, WRITE_CALL,
};

fn rewound(events: &[LiveEvent]) -> Option<&Result<Rewound, krad_chat::RewindError>> {
    events.iter().find_map(|event| match event {
        LiveEvent::Rewound(outcome) => Some(outcome),
        _ => None,
    })
}

fn prompted(events: &[LiveEvent]) -> Vec<RewindPoint> {
    events
        .iter()
        .filter_map(|event| match event {
            LiveEvent::Chat(ChatEvent::Prompted(point)) => Some(point.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn an_edited_second_prompt_is_answered_from_the_first_exchange_only() -> io::Result<()> {
    let root = scratch("rewind-conversation");
    fs::create_dir_all(root.join("work"))?;
    let (url, seen) = fake_gateway(streams(&[ANSWER, ANSWER, ANSWER]))?;
    let id = new_session_id();
    let (chat, events) = start(config(&url, &root), &LiveSession::Start(id.clone()));
    until(&events, |event| matches!(event, LiveEvent::Ready(_)));
    chat.send("first question");
    let first = until(&events, finished);
    chat.send("second question");
    let second = until(&events, finished);
    assert_eq!(prompted(&first), [RewindPoint::Prompt(0)]);
    assert_eq!(prompted(&second), [RewindPoint::Prompt(1)]);
    chat.rewind(RewindRequest {
        point: RewindPoint::Prompt(1),
        scope: RewindScope::Conversation,
        home: None,
    });
    let cut = until(&events, |event| matches!(event, LiveEvent::Rewound(_)));
    assert_eq!(rewound(&cut), Some(&Ok(Rewound::InPlace)));
    chat.send("second question, edited");
    let again = until(&events, finished);
    assert_eq!(prompted(&again), [RewindPoint::Prompt(1)]);
    drop(chat);
    let bodies = completions(&seen);
    assert_eq!(bodies.len(), 3);
    let last = &bodies[2];
    assert!(last.contains("first question"), "{last}");
    assert!(last.contains("second question, edited"), "{last}");
    assert!(!last.contains("\"second question\""), "{last}");
    let entries = read_engine_transcript(&root.join("sessions"), &root.join("work"), &id)?;
    let asked: Vec<&str> = entries
        .iter()
        .filter_map(|entry| match entry {
            ChatEntry::User { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(asked, ["first question", "second question, edited"]);
    Ok(())
}

#[test]
fn a_files_rewind_puts_back_what_the_turn_wrote() -> io::Result<()> {
    let root = scratch("rewind-files");
    let work = root.join("work");
    fs::create_dir_all(&work)?;
    fs::write(work.join("note.txt"), "old\n")?;
    let (url, _) = fake_gateway(streams(&[WRITE_CALL, ANSWER]))?;
    let (chat, events) = start(config(&url, &root), &LiveSession::Start(new_session_id()));
    until(&events, |event| matches!(event, LiveEvent::Ready(_)));
    chat.send("write a note");
    let asked = until(&events, |event| matches!(event, LiveEvent::Ask(_)));
    let Some(LiveEvent::Ask(ask)) = asked.last() else {
        unreachable!("a write in manual mode asks first: {asked:?}");
    };
    chat.answer(ask, &AskAnswer::Allow);
    until(&events, finished);
    assert_eq!(fs::read_to_string(work.join("note.txt"))?, "hi\n");
    chat.rewind(RewindRequest {
        point: RewindPoint::Prompt(0),
        scope: RewindScope::ConversationAndFiles,
        home: None,
    });
    let cut = until(&events, |event| matches!(event, LiveEvent::Rewound(_)));
    assert_eq!(rewound(&cut), Some(&Ok(Rewound::InPlace)));
    assert_eq!(fs::read_to_string(work.join("note.txt"))?, "old\n");
    chat.rewind(RewindRequest {
        point: RewindPoint::Prompt(0),
        scope: RewindScope::Conversation,
        home: None,
    });
    let refused = until(&events, |event| matches!(event, LiveEvent::Rewound(_)));
    assert!(
        matches!(rewound(&refused), Some(Err(_))),
        "nothing left to cut"
    );
    Ok(())
}
