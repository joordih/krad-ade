use super::*;
use crate::command::Command;
use crate::event::{Event, EventKind};
use crate::ids::Seq;
use krad_domain::{EnvironmentName, Speaker, TerminalId};

fn chat() -> AgentRef {
    AgentRef {
        environment: EnvironmentName::new("main".to_owned()),
        terminal: TerminalId(4),
    }
}

fn message(text: &str) -> ChatMessage {
    ChatMessage::new(Speaker::Assistant, text.to_owned())
}

#[test]
fn an_answer_to_an_ask_has_a_stable_wire_shape() -> serde_json::Result<()> {
    let command = Command::Chat(ChatCommand::AnswerAsk {
        chat: chat(),
        ask: ChatAskId(7),
        answer: AskAnswer::Allow,
    });
    let json = serde_json::to_string(&command)?;
    assert_eq!(
        json,
        r#"{"area":"chat","verb":"answer_ask","chat":{"environment":"main","terminal":4},"ask":7,"answer":"allow"}"#
    );
    assert_eq!(serde_json::from_str::<Command>(&json)?, command);
    Ok(())
}

#[test]
fn a_newer_chat_verb_reads_as_unknown() -> serde_json::Result<()> {
    let command: Command = serde_json::from_str(r#"{"area":"chat","verb":"branch","at":3}"#)?;
    assert_eq!(command, Command::Chat(ChatCommand::Unknown));
    Ok(())
}

#[test]
fn chat_events_round_trip() -> serde_json::Result<()> {
    let kinds = [
        EventKind::ChatOpened {
            chat: chat(),
            state: Box::new(ChatState::new(AgentPreset::ClaudeCode)),
        },
        EventKind::ChatMessageAdded {
            chat: chat(),
            index: 0,
            message: Box::new(message("hi")),
        },
        EventKind::ChatMessageChanged {
            chat: chat(),
            index: 0,
            message: Box::new(message("hi there")),
        },
        EventKind::ChatMessagesCut {
            chat: chat(),
            len: 0,
        },
        EventKind::ChatAsk(ChatAsk {
            chat: chat(),
            ask: ChatAskId(1),
            kind: AskKind::Plan {
                plan: "go".to_owned(),
            },
            can_allow_always: false,
        }),
        EventKind::ChatAskSettled {
            chat: chat(),
            ask: ChatAskId(1),
        },
        EventKind::ChatClosed { chat: chat() },
    ];
    for kind in kinds {
        let event = Event { seq: Seq(3), kind };
        let json = serde_json::to_string(&event)?;
        assert_eq!(serde_json::from_str::<Event>(&json)?, event);
    }
    Ok(())
}

#[test]
fn a_mirror_follows_added_changed_and_cut_messages_and_asks() {
    let mut mirror = ChatMirror::new(ChatState::new(AgentPreset::ClaudeCode));
    let added = |index, text: &str| EventKind::ChatMessageAdded {
        chat: chat(),
        index,
        message: Box::new(message(text)),
    };
    assert_eq!(mirror.apply(&added(0, "a")), Some(MirrorChange::Message(0)));
    assert_eq!(mirror.apply(&added(1, "b")), Some(MirrorChange::Message(1)));
    assert_eq!(mirror.apply(&added(5, "gap")), None);
    let changed = EventKind::ChatMessageChanged {
        chat: chat(),
        index: 0,
        message: Box::new(message("a!")),
    };
    assert_eq!(mirror.apply(&changed), Some(MirrorChange::Message(0)));
    assert_eq!(
        mirror.apply(&EventKind::ChatMessagesCut {
            chat: chat(),
            len: 1
        }),
        Some(MirrorChange::Cut(1))
    );
    assert_eq!(mirror.messages, [message("a!")]);
    let ask = ChatAsk {
        chat: chat(),
        ask: ChatAskId(2),
        kind: AskKind::Plan {
            plan: "go".to_owned(),
        },
        can_allow_always: false,
    };
    assert_eq!(
        mirror.apply(&EventKind::ChatAsk(ask)),
        Some(MirrorChange::Asks)
    );
    let settled = EventKind::ChatAskSettled {
        chat: chat(),
        ask: ChatAskId(2),
    };
    assert_eq!(mirror.apply(&settled), Some(MirrorChange::Asks));
    assert_eq!(mirror.apply(&settled), None);
}

#[test]
fn a_mirror_from_a_tail_takes_earlier_pages_in_front() {
    let mut mirror = ChatMirror::from(ChatSummary {
        state: ChatState::new(AgentPreset::ClaudeCode),
        tail: ChatPage {
            chat: chat(),
            len: 3,
            from: 2,
            messages: vec![message("c")],
        },
        asks: Vec::new(),
    });
    mirror.prepend(ChatPage {
        chat: chat(),
        len: 3,
        from: 0,
        messages: vec![message("a"), message("b")],
    });
    assert_eq!(mirror.from, 0);
    assert_eq!(mirror.messages, [message("a"), message("b"), message("c")]);
    mirror.apply(&EventKind::ChatMessagesCut {
        chat: chat(),
        len: 0,
    });
    assert!(mirror.is_empty());
}
