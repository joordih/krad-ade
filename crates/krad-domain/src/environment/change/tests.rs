use super::*;
use crate::agent::AgentState;
use crate::environment::seed_environment;
use crate::group::SessionSpec;
use std::path::PathBuf;

fn replayed(before: &EnvironmentData, after: &EnvironmentData) -> EnvironmentData {
    let mut mirror = before.clone();
    for change in EnvironmentChange::between(before, after) {
        mirror.apply(change);
    }
    mirror
}

#[test]
fn the_same_environment_has_no_changes() {
    let environment = seed_environment();
    assert_eq!(
        EnvironmentChange::between(&environment, &environment.clone()),
        Vec::new()
    );
}

#[test]
fn one_node_that_changes_state_is_one_change_with_its_index() {
    let before = seed_environment();
    let mut after = before.clone();
    after.nodes[1].state = AgentState::Working;
    let changes = EnvironmentChange::between(&before, &after);
    assert_eq!(
        changes,
        vec![EnvironmentChange::Node {
            index: 1,
            node: after.nodes[1].clone(),
        }]
    );
    assert_eq!(replayed(&before, &after), after);
}

#[test]
fn added_removed_and_moved_items_replay_into_the_same_environment() {
    let mut before = seed_environment();
    let api = before.add_group(PathBuf::from("/work/api"), None);
    before.add_group(PathBuf::from("/work/web"), None);
    before.enter_sessions();
    before.new_session(Some(api), SessionSpec::default());
    before.meta.insert(
        TerminalId(1),
        TerminalMeta {
            cwd: "/work/api".to_owned(),
            scrollback: vec!["$ ls".to_owned()],
        },
    );
    let mut after = before.clone();
    after.nodes.remove(0);
    after.nodes.reverse();
    after.groups.retain(|group| group.id != api);
    let docs = after.add_group(PathBuf::from("/work/docs"), None);
    after.new_session(Some(docs), SessionSpec::default());
    after.meta.remove(&TerminalId(1));
    after.meta.insert(
        TerminalId(3),
        TerminalMeta {
            cwd: "/tmp".to_owned(),
            scrollback: Vec::new(),
        },
    );
    after.title = Some("Payments".to_owned());
    assert_eq!(replayed(&before, &after), after);
}

#[test]
fn a_change_round_trips_through_json() -> serde_json::Result<()> {
    let change = EnvironmentChange::NodeRemoved { id: TerminalId(4) };
    let json = serde_json::to_string(&change)?;
    assert_eq!(json, r#"{"change":"node_removed","id":4}"#);
    assert_eq!(serde_json::from_str::<EnvironmentChange>(&json)?, change);
    Ok(())
}

#[test]
fn only_terminal_meta_belongs_to_the_runtime() {
    let meta = EnvironmentChange::Meta {
        terminal: TerminalId(1),
        meta: TerminalMeta {
            cwd: "/work".to_owned(),
            scrollback: Vec::new(),
        },
    };
    assert!(meta.is_runtime_owned());
    assert!(EnvironmentChange::MetaRemoved {
        terminal: TerminalId(1)
    }
    .is_runtime_owned());
    assert!(!EnvironmentChange::NodeRemoved { id: TerminalId(1) }.is_runtime_owned());
    assert!(!EnvironmentChange::Title { title: None }.is_runtime_owned());
}
