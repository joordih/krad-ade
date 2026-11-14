use super::*;
use krad_domain::{AgentState, EnvironmentEdit, NodeEdit, NodeField, SessionEdit, TerminalId};

const MAIN: &str = "main";
const SIDE: &str = "side";
const SHOWN: &str = "the environment is shown";

fn name(text: &str) -> EnvironmentName {
    EnvironmentName::new(text.to_owned())
}

fn environment(text: &str) -> EnvironmentData {
    let mut data = EnvironmentData::empty(name(text));
    data.enter_sessions();
    data
}

#[test]
fn an_edit_lands_in_the_shown_copy_the_shell_holds_without_copying_it() -> Result<(), &'static str>
{
    let mut shown = ShownEnvironments::default();
    shown.replace(vec![environment(MAIN)]);
    let mut held = shown.get(&name(MAIN)).cloned().ok_or(SHOWN)?;
    let step: EnvironmentEdit = SessionEdit::Step { forward: true }.into();
    let before = Rc::as_ptr(&held);
    shown
        .edit(&name(MAIN), &mut held, |copy| {
            copy.edit(&step, RequestId(1))
        })
        .ok_or(SHOWN)?;
    assert_eq!(Rc::as_ptr(&held), before);
    assert!(Rc::ptr_eq(&held, shown.get(&name(MAIN)).ok_or(SHOWN)?));
    assert_eq!(Rc::strong_count(&held), 2);
    Ok(())
}

#[test]
fn an_optimistic_open_is_adopted_when_the_runtime_opens_it() -> Result<(), &'static str> {
    let mut shown = ShownEnvironments::default();
    shown.replace(vec![environment(MAIN)]);
    shown.open(environment(SIDE));
    assert_eq!(shown.names(), [name(MAIN), name(SIDE)]);
    let mut runtime = environment(SIDE);
    runtime.next_id = 9;
    assert!(shown.opened(&runtime).is_empty());
    assert_eq!(shown.get(&name(SIDE)).ok_or(SHOWN)?.next_id, 9);
    shown.ordered(&[name(SIDE), name(MAIN)]);
    assert_eq!(shown.names(), [name(SIDE), name(MAIN)]);
    shown.closed(&name(SIDE));
    assert_eq!(shown.names(), [name(MAIN)]);
    Ok(())
}

#[test]
fn a_runtime_change_reaches_the_shown_copy_and_tells_what_moved() -> Result<(), &'static str> {
    let mut shown = ShownEnvironments::default();
    shown.replace(vec![environment(MAIN)]);
    let mut runtime = environment(MAIN);
    let node = krad_domain::TerminalNode {
        id: TerminalId(4),
        title: "terminal".to_owned(),
        state: AgentState::Idle,
        harness: None,
        resume: None,
        group: None,
        text_scale: krad_domain::TextScale::default(),
        kind: krad_domain::NodeKind::Terminal,
        file: None,
        extension: None,
        bookmarks: krad_domain::ChatBookmarks::default(),
        usage: krad_domain::ChatUsage::default(),
        checkpoints: krad_domain::ChatCheckpoints::default(),
        choice: krad_domain::ChatChoice::default(),
        cloud: None,
    };
    let opened: EnvironmentEdit = NodeEdit::Open {
        node: Box::new(node),
        place: krad_domain::NodePlace::Tab { anchor: None },
    }
    .into();
    let (_, changes) = runtime.edit_with_changes(&opened);
    let moved = shown.heard(&name(MAIN), &changes, None, &runtime);
    assert_eq!(moved.came, vec![TerminalId(4)]);
    let working: EnvironmentEdit = NodeEdit::Set {
        terminal: TerminalId(4),
        field: NodeField::State(AgentState::Working),
    }
    .into();
    let (_, changes) = runtime.edit_with_changes(&working);
    assert!(shown
        .heard(&name(MAIN), &changes, None, &runtime)
        .is_empty());
    assert_eq!(
        shown.get(&name(MAIN)).ok_or(SHOWN)?.nodes,
        runtime.nodes,
        "the shown copy follows what the runtime changed"
    );
    Ok(())
}
