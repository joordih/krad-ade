use futures::executor::block_on;
use futures::StreamExt as _;
use krad_domain::{
    AgentState, BudgetStatus, EnvironmentChange, EnvironmentData, EnvironmentName, ReportedUsage,
    RuntimeState, SessionSpec, SpendSettings, TerminalId, TokenUsage, UsageReport, UsdCost,
};
use krad_protocol::{
    Command, EnvironmentCommand, ErrorCode, EventKind, Feed, Reply, RuntimeCommand, Scope,
    SpendingCrossed,
};
use krad_runtime::{EnvironmentsSetup, EventReceiver, Runtime, RuntimeHandle, RuntimeSetup};
use krad_store::{open_layout_in, LayoutFile};
use std::error::Error;
use std::path::{Path, PathBuf};

type TestResult = Result<(), Box<dyn Error>>;

const FEED_ENDED: &str = "the feed ended";
const NOT_A_SNAPSHOT: &str = "the feed did not start from a snapshot";
const CHAT: TerminalId = TerminalId(1);

struct Folder(PathBuf);

impl Folder {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "krad-runtime-environments-{name}-{}",
            std::process::id()
        ));
        drop(std::fs::remove_dir_all(&dir));
        Self(dir)
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.0));
    }
}

fn name(raw: &str) -> EnvironmentName {
    EnvironmentName::new(raw.to_owned())
}

fn start(state: RuntimeState, dir: &Path) -> Result<RuntimeHandle, Box<dyn Error>> {
    Ok(Runtime::start(RuntimeSetup {
        environments: EnvironmentsSetup {
            state,
            file: Some(LayoutFile::State.path_in(dir)),
        },
        ..RuntimeSetup::default()
    })?)
}

fn send(runtime: &RuntimeHandle, command: EnvironmentCommand) -> Result<Reply, Box<dyn Error>> {
    Ok(block_on(runtime.send(
        runtime.next_request(),
        Command::Environments(command),
    ))?)
}

fn subscribed(
    runtime: &RuntimeHandle,
) -> Result<(EventReceiver, Vec<EnvironmentData>), Box<dyn Error>> {
    let mut events = runtime.subscribe();
    match block_on(events.next()).ok_or(FEED_ENDED)? {
        Feed::Snapshot(snapshot) => Ok((events, snapshot.environments)),
        Feed::Event(_) => Err(NOT_A_SNAPSHOT.into()),
    }
}

fn events_until_pong(
    runtime: &RuntimeHandle,
    events: &mut EventReceiver,
) -> Result<Vec<EventKind>, Box<dyn Error>> {
    let marker = runtime.next_request();
    drop(block_on(
        runtime.send(marker, Command::Runtime(RuntimeCommand::Ping)),
    )?);
    let mut heard = Vec::new();
    loop {
        match block_on(events.next()).ok_or(FEED_ENDED)? {
            Feed::Event(event) => match event.kind {
                EventKind::Pong { request } if request == marker => return Ok(heard),
                kind => heard.push(kind),
            },
            Feed::Snapshot(_) => return Err(NOT_A_SNAPSHOT.into()),
        }
    }
}

fn mirrored(mut mirror: Vec<EnvironmentData>, events: Vec<EventKind>) -> Vec<EnvironmentData> {
    for kind in events {
        match kind {
            EventKind::EnvironmentOpened { environment } => {
                match mirror.iter_mut().find(|kept| kept.name == environment.name) {
                    Some(kept) => *kept = *environment,
                    None => mirror.push(*environment),
                }
            }
            EventKind::EnvironmentClosed { environment } => {
                mirror.retain(|kept| kept.name != environment);
            }
            EventKind::EnvironmentsOrdered { order } => {
                mirror.sort_by_key(|kept| order.iter().position(|name| *name == kept.name));
            }
            EventKind::EnvironmentChanged {
                environment,
                changes,
            } => {
                if let Some(kept) = mirror.iter_mut().find(|kept| kept.name == environment) {
                    for change in changes {
                        kept.apply(change);
                    }
                }
            }
            _ => {}
        }
    }
    mirror
}

fn payments() -> Result<EnvironmentData, Box<dyn Error>> {
    let mut environment = EnvironmentData::empty(name("payments"));
    let group = environment.add_group(PathBuf::from("/work/api"), None);
    environment
        .nodes
        .push(serde_json::from_value(serde_json::json!({
            "id": CHAT.0, "title": "claude", "state": "Idle", "kind": "Chat", "group": group.0
        }))?);
    environment.next_id = CHAT.0 + 1;
    environment.enter_sessions();
    environment.new_session(Some(group), SessionSpec::default());
    Ok(environment)
}

fn edited(before: &EnvironmentData) -> EnvironmentData {
    let mut after = before.clone();
    after.title = Some("Payments".to_owned());
    if let Some(node) = after.nodes.first_mut() {
        node.state = AgentState::Working;
    }
    after.add_group(PathBuf::from("/work/web"), None);
    after
}

#[test]
fn environment_commands_persist_and_a_restarted_runtime_restores_them() -> TestResult {
    let folder = Folder::new("restart");
    let runtime = start(RuntimeState::default(), &folder.0)?;
    let opened = payments()?;
    let changed = edited(&opened);
    let scratch = EnvironmentData::empty(name("scratch"));
    for command in [
        EnvironmentCommand::Open {
            environment: Box::new(opened.clone()),
        },
        EnvironmentCommand::Open {
            environment: Box::new(scratch.clone()),
        },
        EnvironmentCommand::Change {
            environment: opened.name.clone(),
            changes: EnvironmentChange::between(&opened, &changed),
        },
        EnvironmentCommand::Order {
            order: vec![scratch.name.clone(), opened.name.clone()],
        },
    ] {
        assert_eq!(send(&runtime, command)?, Reply::Accepted);
    }
    let expected = vec![scratch, changed];
    block_on(runtime.settle())?;
    let state = open_layout_in(&folder.0).layout.state;
    assert_eq!(state.environments, expected);
    drop(runtime);
    let restarted = start(state, &folder.0)?;
    let (_, restored) = subscribed(&restarted)?;
    assert_eq!(restored, expected);
    assert_eq!(
        block_on(restarted.snapshot(Scope::Runtime))?.environments,
        expected
    );
    Ok(())
}

#[test]
fn the_event_stream_rebuilds_the_runtime_state_on_a_mirror() -> TestResult {
    let folder = Folder::new("mirror");
    let seeded = payments()?;
    let runtime = start(
        RuntimeState {
            environments: vec![seeded.clone()],
            ..RuntimeState::default()
        },
        &folder.0,
    )?;
    let (mut events, start_state) = subscribed(&runtime)?;
    assert_eq!(start_state, vec![seeded.clone()]);
    let changed = edited(&seeded);
    let mut removed = changed.clone();
    removed.groups.remove(0);
    for command in [
        EnvironmentCommand::Change {
            environment: seeded.name.clone(),
            changes: EnvironmentChange::between(&seeded, &changed),
        },
        EnvironmentCommand::Open {
            environment: Box::new(EnvironmentData::empty(name("docs"))),
        },
        EnvironmentCommand::Change {
            environment: seeded.name.clone(),
            changes: EnvironmentChange::between(&changed, &removed),
        },
        EnvironmentCommand::Order {
            order: vec![name("docs")],
        },
        EnvironmentCommand::Close {
            environment: name("docs"),
        },
    ] {
        assert_eq!(send(&runtime, command)?, Reply::Accepted);
    }
    let heard = events_until_pong(&runtime, &mut events)?;
    assert!(heard.iter().all(|kind| !matches!(
        kind,
        EventKind::EnvironmentChanged { changes, .. } if changes.len() > 3
    )));
    let snapshot = block_on(runtime.snapshot(Scope::Runtime))?.environments;
    assert_eq!(mirrored(start_state, heard), snapshot);
    assert_eq!(snapshot, vec![removed]);
    Ok(())
}

#[test]
fn a_change_to_an_unknown_environment_is_refused() -> TestResult {
    let folder = Folder::new("unknown");
    let runtime = start(RuntimeState::default(), &folder.0)?;
    let reply = send(
        &runtime,
        EnvironmentCommand::Change {
            environment: name("nowhere"),
            changes: vec![EnvironmentChange::NextId { next: 2 }],
        },
    )?;
    assert!(matches!(
        reply,
        Reply::Refused {
            code: ErrorCode::NotFound,
            ..
        }
    ));
    Ok(())
}

fn spent(cents: u64) -> UsageReport {
    UsageReport::Added(ReportedUsage {
        tokens: TokenUsage::default(),
        cost: Some(UsdCost::from_nanos(cents * NANOS_PER_CENT)),
        model: None,
    })
}

const NANOS_PER_CENT: u64 = 10_000_000;
const BUDGET_CENTS: u64 = 100;

#[test]
fn recorded_usage_reaches_the_chat_and_a_crossed_budget_is_told_once() -> TestResult {
    let folder = Folder::new("spend");
    let environment = payments()?;
    let runtime = start(
        RuntimeState {
            environments: vec![environment.clone()],
            ..RuntimeState::default()
        },
        &folder.0,
    )?;
    let (mut events, _) = subscribed(&runtime)?;
    let spending = SpendSettings {
        group_budget: Some(UsdCost::from_nanos(BUDGET_CENTS * NANOS_PER_CENT)),
        ..SpendSettings::default()
    };
    let record = |cents| EnvironmentCommand::RecordUsage {
        environment: environment.name.clone(),
        chat: CHAT,
        report: spent(cents),
        spending: spending.clone(),
    };
    assert_eq!(send(&runtime, record(BUDGET_CENTS * 2))?, Reply::Accepted);
    assert_eq!(send(&runtime, record(1))?, Reply::Accepted);
    let heard = events_until_pong(&runtime, &mut events)?;
    let crossed: Vec<&SpendingCrossed> = heard
        .iter()
        .filter_map(|kind| match kind {
            EventKind::SpendingCrossed(crossed) => Some(crossed),
            _ => None,
        })
        .collect();
    assert_eq!(crossed.len(), 1);
    assert_eq!(crossed[0].status, BudgetStatus::Exceeded);
    let snapshot = block_on(runtime.snapshot(Scope::Runtime))?.environments;
    assert_eq!(
        snapshot[0].chat_spend(CHAT).cost,
        UsdCost::from_nanos((BUDGET_CENTS * 2 + 1) * NANOS_PER_CENT)
    );
    Ok(())
}
