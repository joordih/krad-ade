use krad_domain::{
    AgentState, ChatBookmarks, ChatCheckpoints, ChatChoice, ChatUsage, EnvironmentChange,
    EnvironmentName, NodeKind, TerminalId, TerminalNode, TextScale,
};
use krad_protocol::{EventKind, Feed};

use super::super::tests::snapshot;
use super::super::{Heard, ViewModel};

const MAIN: &str = "main";

fn chat(id: u64) -> TerminalNode {
    TerminalNode {
        id: TerminalId(id),
        title: "chat".to_owned(),
        state: AgentState::Idle,
        harness: None,
        resume: None,
        group: None,
        text_scale: TextScale::default(),
        kind: NodeKind::Chat,
        file: None,
        extension: None,
        bookmarks: ChatBookmarks::default(),
        usage: ChatUsage::default(),
        checkpoints: ChatCheckpoints::default(),
        choice: ChatChoice::default(),
        cloud: None,
    }
}

#[test]
fn the_view_model_keeps_the_shown_environments_and_tells_the_shell_what_moved() {
    let main = EnvironmentName::new(MAIN.to_owned());
    let mut model = ViewModel::default();
    model.feed(Feed::Snapshot(snapshot()));
    assert_eq!(model.shown.names(), model.environments.names());
    drop(model.take_news());
    model.fold_event(EventKind::EnvironmentChanged {
        environment: main.clone(),
        changes: vec![EnvironmentChange::Node {
            index: 0,
            node: chat(5),
        }],
        answers: None,
    });
    let shown = model.shown.get(&main).map(|copy| copy.nodes.clone());
    assert_eq!(shown, Some(vec![chat(5)]));
    let moved = model
        .take_news()
        .heard
        .into_iter()
        .find_map(|heard| match heard {
            Heard::NodesMoved { environment, moved } => Some((environment, moved.came)),
            _ => None,
        });
    assert_eq!(moved, Some((main, vec![TerminalId(5)])));
}

#[test]
fn the_view_model_keeps_each_chat_s_read_copy_from_the_snapshot_and_its_events() {
    let opened = krad_protocol::AgentRef {
        environment: EnvironmentName::new(MAIN.to_owned()),
        terminal: TerminalId(5),
    };
    let mut model = ViewModel::default();
    let mut state = krad_protocol::ChatState::new(krad_domain::AgentPreset::Codex);
    let mut known = snapshot();
    known.chats = vec![krad_protocol::ChatSummary {
        state: state.clone(),
        tail: krad_protocol::ChatPage {
            chat: opened.clone(),
            len: 0,
            from: 0,
            messages: Vec::new(),
        },
        asks: Vec::new(),
    }];
    model.feed(Feed::Snapshot(known));
    assert!(model.chats.contains(&opened));
    state.queued = vec![krad_domain::Prompt::from("go on")];
    model.fold_event(EventKind::ChatState {
        chat: opened.clone(),
        state: Box::new(state.clone()),
    });
    assert_eq!(
        model.chats.get(&opened).map(|mirror| &mirror.state),
        Some(&state)
    );
    let heard = model.take_news().heard;
    assert!(
        heard
            .iter()
            .any(|heard| matches!(heard, Heard::Event(EventKind::ChatState { .. }))),
        "the shell still hears the event"
    );
    model.fold_event(EventKind::ChatClosed {
        chat: opened.clone(),
    });
    assert!(!model.chats.contains(&opened));
}
