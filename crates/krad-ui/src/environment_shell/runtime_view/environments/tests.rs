use super::*;
use krad_domain::{EnvironmentView, TerminalId};
use std::path::PathBuf;

fn named(raw: &str) -> EnvironmentData {
    EnvironmentData::empty(EnvironmentName::new(raw.to_owned()))
}

fn heard(mirror: &mut EnvironmentsMirror, commands: Vec<EnvironmentCommand>) {
    for command in commands {
        match command {
            EnvironmentCommand::Open { environment } => mirror.opened(*environment),
            EnvironmentCommand::Close { environment } => mirror.closed(&environment),
            EnvironmentCommand::Order { order } => mirror.ordered(&order),
            EnvironmentCommand::Change {
                environment,
                changes,
            } => mirror.changed(&environment, changes),
            EnvironmentCommand::RecordUsage { .. } | EnvironmentCommand::Unknown => {}
        }
    }
}

#[test]
fn a_mirror_in_step_with_the_shell_sends_nothing() {
    let mut mirror = EnvironmentsMirror::default();
    let environments = vec![named("main"), named("docs")];
    mirror.replace(environments.clone());
    assert_eq!(mirror.commands_toward(&environments), Vec::new());
}

#[test]
fn opened_closed_reordered_and_changed_environments_reach_the_same_state() {
    let mut mirror = EnvironmentsMirror::default();
    let mut main = named("main");
    main.add_group(PathBuf::from("/work/api"), None);
    mirror.replace(vec![main.clone(), named("old"), named("docs")]);
    let mut changed = main.clone();
    changed.title = Some("Payments".to_owned());
    changed.add_group(PathBuf::from("/work/web"), None);
    let wanted = vec![named("docs"), named("new"), changed];
    let commands = mirror.commands_toward(&wanted);
    assert!(commands
        .iter()
        .any(|command| matches!(command, EnvironmentCommand::Close { environment } if environment.as_str() == "old")));
    heard(&mut mirror, commands);
    assert_eq!(mirror.0, wanted);
    assert_eq!(mirror.commands_toward(&wanted), Vec::new());
}

#[test]
fn a_new_environment_appended_last_needs_no_reordering() {
    let mut mirror = EnvironmentsMirror::default();
    mirror.replace(vec![named("main")]);
    let commands = mirror.commands_toward(&[named("main"), named("docs")]);
    assert_eq!(commands.len(), 1);
    assert!(matches!(commands[0], EnvironmentCommand::Open { .. }));
}

#[test]
fn an_opened_environment_leaves_its_view_to_the_shell() {
    let mut mirror = EnvironmentsMirror::default();
    let mut shown = named("main");
    shown.view = EnvironmentView::Notes;
    shown.focus = Some(TerminalId(3));
    let commands = mirror.commands_toward(std::slice::from_ref(&shown));
    let Some(EnvironmentCommand::Open { environment }) = commands.first() else {
        unreachable!("a new environment is opened");
    };
    assert_eq!(environment.view, EnvironmentView::Sessions);
    assert_eq!(environment.focus, None);
    heard(&mut mirror, commands);
    let mut working = shown.clone();
    working.view = EnvironmentView::Review;
    working.focus = None;
    assert_eq!(mirror.commands_toward(&[working]), Vec::new());
}

#[test]
fn the_ui_never_sends_the_meta_the_runtime_owns() {
    let mut mirror = EnvironmentsMirror::default();
    let mut kept = named("main");
    kept.meta.insert(
        TerminalId(3),
        krad_domain::TerminalMeta {
            cwd: "/work/api".to_owned(),
            scrollback: vec!["from the runtime".to_owned()],
        },
    );
    mirror.replace(vec![kept.clone()]);
    let mut stale = kept;
    stale.meta.clear();
    assert_eq!(
        mirror.commands_toward(std::slice::from_ref(&stale)),
        Vec::new()
    );
    stale.title = Some("Payments".to_owned());
    let commands = mirror.commands_toward(&[stale]);
    assert!(matches!(
        commands.as_slice(),
        [EnvironmentCommand::Change { changes, .. }]
            if changes.iter().all(|change| !change.is_runtime_owned())
    ));
}
