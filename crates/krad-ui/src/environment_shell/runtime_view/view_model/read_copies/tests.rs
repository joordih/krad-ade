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
