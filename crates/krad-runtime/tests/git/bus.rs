use super::repo::{committed_repo, TRACKED};
use super::{main_name, until, TestResult, CHANGED_TEXT};
use futures::executor::block_on;
use futures::StreamExt as _;
use krad_agents::{call, Caller, Request, Response, TerminalHandle};
use krad_domain::{EnvironmentData, RuntimeState, TerminalId};
use krad_protocol::EventKind;
use krad_runtime::{BusSetup, EnvironmentsSetup, Runtime, RuntimeSetup};
use std::path::Path;
use std::sync::mpsc::channel;

const TERMINAL: TerminalId = TerminalId(1);
const NO_BUS: &str = "the runtime serves no bus";

#[test]
fn a_terminal_reads_its_groups_changes_and_branches_a_worktree_through_the_runtime() -> TestResult {
    let repo = committed_repo("git-bus")?;
    std::fs::write(repo.0.join(TRACKED), CHANGED_TEXT)?;
    let mut data = EnvironmentData::empty(main_name());
    let group = data.add_group(repo.0.clone(), None);
    data.nodes = vec![serde_json::from_value(serde_json::json!({
        "id": TERMINAL.0, "title": "agent", "state": "Idle", "group": group.0
    }))?];
    let runtime = Runtime::start(RuntimeSetup {
        bus: Some(BusSetup::default()),
        environments: EnvironmentsSetup {
            state: RuntimeState {
                environments: vec![data],
                ..RuntimeState::default()
            },
            file: None,
        },
        ..RuntimeSetup::default()
    })?;
    let mut feed = runtime.subscribe();
    let (heard, received) = channel();
    std::thread::spawn(move || {
        while let Some(next) = block_on(feed.next()) {
            if heard.send(next).is_err() {
                break;
            }
        }
    });
    until(
        &received,
        |kind| matches!(kind, EventKind::GitRead(read) if read.root == repo.0 && read.git.is_some()),
    )?;
    let endpoint = &runtime.bus().ok_or(NO_BUS)?.endpoint;
    let caller = Caller::Terminal(TerminalHandle {
        environment: main_name().as_str().to_owned(),
        terminal: TERMINAL.0,
    });
    let asked = |request| {
        call(
            &endpoint.address.to_string(),
            &endpoint.token,
            &caller,
            request,
        )
    };
    let Response::Changes(changes) = asked(Request::Changes { session: None })? else {
        return Err("the group's changes were not read".into());
    };
    assert_eq!(changes.root, repo.0);
    assert_eq!(changes.branch.as_deref(), Some("main"));
    assert!(changes
        .files
        .iter()
        .any(|file| file.path == Path::new(TRACKED)));
    assert!(changes.patch.contains("+second"));
    let made = asked(Request::CreateWorktree {
        name: "bus try".to_owned(),
    })?;
    let Response::WorktreeCreated(info) = made else {
        return Err(format!("no worktree was made: {made:?}").into());
    };
    assert_eq!(info.branch.as_deref(), Some("bus-try"));
    assert!(info.path.is_dir());
    let refused = asked(Request::CreateWorktree {
        name: "..".to_owned(),
    })?;
    assert!(matches!(refused, Response::Error(_)), "{refused:?}");
    Ok(())
}
