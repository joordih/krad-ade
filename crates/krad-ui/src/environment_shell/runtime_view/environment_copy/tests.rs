use super::*;
use krad_domain::{
    AgentState, ChatBookmarks, ChatCheckpoints, ChatChoice, ChatUsage, EnvironmentName, GroupEdit,
    GroupId, NodeEdit, NodeField, NodePlace, SessionEdit, TerminalNode, TextScale,
};
use std::path::PathBuf;

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

fn main() -> EnvironmentData {
    let mut data = EnvironmentData::empty(EnvironmentName::new("main".to_owned()));
    data.nodes.push(chat(1));
    data.next_id = 2;
    data.enter_sessions();
    data
}

fn state_of(data: &EnvironmentData) -> Option<AgentState> {
    data.nodes.first().map(|node| node.state)
}

#[test]
fn an_optimistic_edit_holds_until_its_answer_then_the_runtime_state_wins() {
    let mut runtime = main();
    let mut copy = EnvironmentCopy::new(runtime.clone());
    let working: EnvironmentEdit = NodeEdit::Set {
        terminal: TerminalId(1),
        field: NodeField::State(AgentState::Working),
    }
    .into();
    let added: EnvironmentEdit = GroupEdit::Add {
        group: GroupId(1),
        root: PathBuf::from("/work/api"),
        worktree: None,
    }
    .into();
    copy.edit(&working, RequestId(1));
    copy.edit(&added, RequestId(2));
    let (_, first) = runtime.edit_with_changes(&working);
    assert!(copy.heard(&first, Some(RequestId(1)), &runtime).is_empty());
    assert_eq!(
        copy.groups.len(),
        1,
        "the later edit is not undone by an earlier echo"
    );
    assert!(copy.awaiting.is_some());
    let (_, second) = runtime.edit_with_changes(&added);
    copy.heard(&second, Some(RequestId(2)), &runtime);
    assert!(copy.awaiting.is_none());
    assert_eq!(copy.nodes, runtime.nodes);
    assert_eq!(copy.groups, runtime.groups);
    assert_eq!(state_of(&copy), Some(AgentState::Working));
}

#[test]
fn a_quiet_copy_follows_runtime_edits_and_tells_the_nodes_that_came_and_went() {
    let mut runtime = main();
    let mut copy = EnvironmentCopy::new(runtime.clone());
    let worker = chat(7);
    let opened: EnvironmentEdit = NodeEdit::Open {
        node: Box::new(worker),
        place: NodePlace::Tab { anchor: None },
    }
    .into();
    let (_, changes) = runtime.edit_with_changes(&opened);
    let came = copy.heard(&changes, None, &runtime);
    assert_eq!(came.came, vec![TerminalId(7)]);
    assert_eq!(came.gone, Vec::<TerminalId>::new());
    let closed: EnvironmentEdit = NodeEdit::Close {
        terminal: TerminalId(1),
    }
    .into();
    let (_, changes) = runtime.edit_with_changes(&closed);
    let gone = copy.heard(&changes, None, &runtime);
    assert_eq!(gone.gone, vec![TerminalId(1)]);
    assert_eq!(copy.nodes, runtime.nodes);
    assert_eq!(copy.sessions, runtime.sessions);
}

#[test]
fn a_snapshot_is_adopted_only_when_no_edit_waits() {
    let runtime = main();
    let mut copy = EnvironmentCopy::new(runtime.clone());
    let renamed: EnvironmentEdit = SessionEdit::Step { forward: true }.into();
    copy.edit(&renamed, RequestId(4));
    let mut later = runtime;
    later.next_id = 40;
    assert!(copy.adopt(&later).is_empty());
    assert_eq!(copy.next_id, 2);
    copy.heard(&[], Some(RequestId(4)), &later);
    assert_eq!(copy.next_id, 40);
}

#[test]
fn a_runtime_edit_heard_while_an_edit_waits_is_adopted_with_the_answer() {
    let mut runtime = main();
    let mut copy = EnvironmentCopy::new(runtime.clone());
    let scaled: EnvironmentEdit = NodeEdit::Set {
        terminal: TerminalId(1),
        field: NodeField::TextScale(TextScale::default().bigger()),
    }
    .into();
    copy.edit(&scaled, RequestId(5));
    let worker: EnvironmentEdit = NodeEdit::Open {
        node: Box::new(chat(9)),
        place: NodePlace::Tab { anchor: None },
    }
    .into();
    let (_, spawned) = runtime.edit_with_changes(&worker);
    assert!(copy.heard(&spawned, None, &runtime).is_empty());
    assert!(copy.nodes.iter().all(|node| node.id != TerminalId(9)));
    let (_, answer) = runtime.edit_with_changes(&scaled);
    let moved = copy.heard(&answer, Some(RequestId(5)), &runtime);
    assert_eq!(moved.came, vec![TerminalId(9)]);
    assert_eq!(copy.nodes, runtime.nodes);
}
