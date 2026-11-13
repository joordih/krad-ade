use super::{answered, call_from, TestResult, ENVIRONMENT, LEAD};
use futures::executor::block_on;
use futures::StreamExt as _;
use krad_agents::{
    CloseOrder, CloseTarget, Closed, ClosedTarget, Request, Response, TerminalState, WorktreeFate,
    WorktreeLeft, WorktreeOutcome,
};
use krad_chat::new_session_id;
use krad_domain::{
    AgentPreset, EnvironmentData, EnvironmentEdit, EnvironmentName, NodeKind, RuntimeState,
    SessionEdit, SessionLead, Settings, Speaker, TerminalId, WhenWorking,
};
use krad_protocol::{AgentRef, Command, EventKind, Feed, Reply, Scope};
use krad_runtime::{
    BusSetup, EnvironmentsSetup, EventReceiver, Runtime, RuntimeHandle, RuntimeSetup, SettingsSetup,
};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[path = "../support/repo.rs"]
mod repo;

const FAKE_CLAUDE: &str = "../krad-chat/tests/fixtures/fake-harness/claude";
const RECORDED_ANSWER: &str = "Two tabs rotate at once.";
const TASK: &str = "Find why the rotation races";
const WAIT: Duration = Duration::from_secs(10);
const TIMED_OUT: &str = "the awaited event never came";

struct Folder(PathBuf);

impl Drop for Folder {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.0));
    }
}

fn lead_environment(root: &Path) -> Result<EnvironmentData, Box<dyn Error>> {
    let mut data = EnvironmentData::empty(EnvironmentName::new(ENVIRONMENT.to_owned()));
    let group = data.add_group(root.to_path_buf(), None);
    data.nodes = vec![serde_json::from_value(serde_json::json!({
        "id": LEAD.0, "title": "lead", "state": "Idle", "kind": "Chat", "group": group.0
    }))?];
    data.next_id = LEAD.0 + 1;
    data.enter_sessions();
    Ok(data)
}

fn start(data: EnvironmentData) -> Result<RuntimeHandle, Box<dyn Error>> {
    let fake = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(FAKE_CLAUDE)
        .canonicalize()?;
    let mut settings = Settings::default();
    settings
        .harness_commands
        .insert(AgentPreset::ClaudeCode, fake.to_string_lossy().into_owned());
    Ok(Runtime::start(RuntimeSetup {
        bus: Some(BusSetup::default()),
        environments: EnvironmentsSetup {
            state: RuntimeState {
                environments: vec![data],
                ..RuntimeState::default()
            },
            file: None,
        },
        settings: SettingsSetup {
            settings,
            file: None,
        },
        ..RuntimeSetup::default()
    })?)
}

fn until(events: &mut EventReceiver, wanted: impl Fn(&EventKind) -> bool) -> TestResult {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        match block_on(events.next()) {
            Some(Feed::Event(event)) if wanted(&event.kind) => return Ok(()),
            Some(_) => {}
            None => break,
        }
    }
    Err(TIMED_OUT.into())
}

fn main_environment(runtime: &RuntimeHandle) -> Result<EnvironmentData, Box<dyn Error>> {
    Ok(block_on(runtime.snapshot(Scope::Runtime))?
        .environments
        .into_iter()
        .next()
        .ok_or("the runtime lost the environment")?)
}

#[test]
fn a_lead_starts_a_worker_and_closes_it() -> TestResult {
    let folder =
        Folder(std::env::temp_dir().join(format!("krad-lead-{}", new_session_id().as_str())));
    std::fs::create_dir_all(&folder.0)?;
    let data = lead_environment(&folder.0)?;
    let session = data
        .active_session
        .ok_or("sessions mode opened no session")?;
    let runtime = start(data)?;
    let mut events = runtime.subscribe();
    let made_lead: Command = (
        EnvironmentName::new(ENVIRONMENT.to_owned()),
        EnvironmentEdit::from(SessionEdit::MakeLead {
            session,
            lead: SessionLead {
                terminal: Some(LEAD),
                ..SessionLead::new(AgentPreset::ClaudeCode, None)
            },
        }),
    )
        .into();
    assert_eq!(
        block_on(runtime.send(runtime.next_request(), made_lead))?,
        Reply::Accepted
    );
    let spawn = Request::Spawn {
        agent: AgentPreset::ClaudeCode,
        worktree: None,
        task: TASK.to_owned(),
    };
    let Response::Spawned { terminal } = answered(call_from(&runtime, LEAD, spawn)?)? else {
        return Err("the lead's worker did not start".into());
    };
    let worker = TerminalId(terminal);
    let started = main_environment(&runtime)?;
    let node = started
        .nodes
        .iter()
        .find(|node| node.id == worker)
        .ok_or("the worker has no node")?;
    assert_eq!(node.kind, NodeKind::Chat);
    let (owner, record) = started.worker(worker).ok_or("the lead does not own it")?;
    assert_eq!(owner, session);
    let worker_session = record.session.ok_or("the worker has no session")?;
    assert!(started.session(worker_session).is_some());
    let chat = AgentRef {
        environment: EnvironmentName::new(ENVIRONMENT.to_owned()),
        terminal: worker,
    };
    until(&mut events, |kind| {
        matches!(kind, EventKind::ChatMessageAdded { chat: heard, message, .. }
            | EventKind::ChatMessageChanged { chat: heard, message, .. }
            if *heard == chat && message.speaker == Speaker::Assistant
                && message.text == RECORDED_ANSWER)
    })?;
    until(&mut events, |kind| {
        matches!(kind, EventKind::ChatState { chat: heard, state }
            if *heard == chat && !state.working)
    })?;
    let order = CloseOrder {
        target: CloseTarget::Agent(worker.0),
        when_working: WhenWorking::Refuse,
        worktree: WorktreeFate::Keep,
    };
    let closed = answered(call_from(&runtime, LEAD, Request::Close(order))?)?;
    assert_eq!(
        closed,
        Response::Closed(vec![Closed {
            target: ClosedTarget::Agent(worker.0),
            worktree: None,
        }])
    );
    until(
        &mut events,
        |kind| matches!(kind, EventKind::ChatClosed { chat: heard } if *heard == chat),
    )?;
    let ended = main_environment(&runtime)?;
    assert!(ended.nodes.iter().all(|node| node.id != worker));
    assert!(ended.session(worker_session).is_none());
    let (_, retired) = ended.worker(worker).ok_or("the lead forgot its worker")?;
    assert!(retired.session.is_none());
    Ok(())
}

fn spawned(runtime: &RuntimeHandle) -> Result<TerminalId, Box<dyn Error>> {
    let spawn = Request::Spawn {
        agent: AgentPreset::ClaudeCode,
        worktree: None,
        task: TASK.to_owned(),
    };
    match answered(call_from(runtime, LEAD, spawn)?)? {
        Response::Spawned { terminal } => Ok(TerminalId(terminal)),
        other => Err(format!("the lead's worker did not start: {other:?}").into()),
    }
}

fn led_runtime(folder: &Folder) -> Result<RuntimeHandle, Box<dyn Error>> {
    let data = lead_environment(&folder.0)?;
    let session = data
        .active_session
        .ok_or("sessions mode opened no session")?;
    let runtime = start(data)?;
    let made_lead: Command = (
        EnvironmentName::new(ENVIRONMENT.to_owned()),
        EnvironmentEdit::from(SessionEdit::MakeLead {
            session,
            lead: SessionLead {
                terminal: Some(LEAD),
                ..SessionLead::new(AgentPreset::ClaudeCode, None)
            },
        }),
    )
        .into();
    block_on(runtime.send(runtime.next_request(), made_lead))?;
    Ok(runtime)
}

#[test]
fn a_lead_waits_reads_its_finished_worker_lists_it_closed_and_reopens_it() -> TestResult {
    let folder =
        Folder(std::env::temp_dir().join(format!("krad-lead-{}", new_session_id().as_str())));
    std::fs::create_dir_all(&folder.0)?;
    let runtime = led_runtime(&folder)?;
    let worker = spawned(&runtime)?;
    let waited = answered(call_from(
        &runtime,
        LEAD,
        Request::Wait {
            target: worker.0,
            timeout_secs: 10,
        },
    )?)?;
    assert_eq!(
        waited,
        Response::Settled {
            target: worker.0,
            state: TerminalState::Running(krad_domain::AgentState::Idle),
        }
    );
    let read = answered(call_from(
        &runtime,
        LEAD,
        Request::Read {
            target: worker.0,
            lines: 20,
        },
    )?)?;
    assert!(
        matches!(&read, Response::Screen(tail) if tail.contains(RECORDED_ANSWER)),
        "{read:?}"
    );
    let retired = main_environment(&runtime)?;
    assert!(retired.nodes.iter().all(|node| node.id != worker));
    let listed = answered(call_from(&runtime, LEAD, Request::ListAll)?)?;
    assert!(
        matches!(&listed, Response::Agents(agents)
        if agents.iter().any(|agent| agent.terminal == worker.0
            && agent.state == TerminalState::Closed)),
        "{listed:?}"
    );
    let sent = answered(call_from(
        &runtime,
        LEAD,
        Request::Send {
            to: worker.0,
            body: "Now add a test".to_owned(),
        },
    )?)?;
    assert_eq!(sent, Response::Submitted { terminal: worker.0 });
    let reopened = main_environment(&runtime)?;
    assert!(reopened.nodes.iter().any(|node| node.id == worker));
    let (_, record) = reopened
        .worker(worker)
        .ok_or("the lead forgot its worker")?;
    assert!(record.session.is_some());
    Ok(())
}

fn git_read(events: &mut EventReceiver, root: &Path) -> TestResult {
    until(
        events,
        |kind| matches!(kind, EventKind::GitRead(read) if read.root == root && read.git.is_some()),
    )
}

#[test]
fn a_lead_starts_a_worker_in_its_own_worktree_and_removes_it_when_closing() -> TestResult {
    let repo = repo::committed_repo("lead-worktree")?;
    let data = lead_environment(&repo.0)?;
    let session = data
        .active_session
        .ok_or("sessions mode opened no session")?;
    let runtime = start(data)?;
    let mut events = runtime.subscribe();
    let made_lead: Command = (
        EnvironmentName::new(ENVIRONMENT.to_owned()),
        EnvironmentEdit::from(SessionEdit::MakeLead {
            session,
            lead: SessionLead {
                terminal: Some(LEAD),
                ..SessionLead::new(AgentPreset::ClaudeCode, None)
            },
        }),
    )
        .into();
    block_on(runtime.send(runtime.next_request(), made_lead))?;
    git_read(&mut events, &repo.0)?;
    let spawn = Request::Spawn {
        agent: AgentPreset::ClaudeCode,
        worktree: Some("race fix".to_owned()),
        task: TASK.to_owned(),
    };
    let Response::Spawned { terminal } = answered(call_from(&runtime, LEAD, spawn)?)? else {
        return Err("the lead's worker did not start".into());
    };
    let worker = TerminalId(terminal);
    let started = main_environment(&runtime)?;
    let group = started
        .nodes
        .iter()
        .find(|node| node.id == worker)
        .and_then(|node| node.group)
        .and_then(|group| started.group(group))
        .ok_or("the worker has no group")?
        .clone();
    let origin = group
        .worktree
        .clone()
        .ok_or("the worker's group is no worktree")?;
    assert_eq!(origin.repo_root, repo.0);
    assert_eq!(origin.branch, "race-fix");
    assert!(group.root.is_dir());
    let waited = answered(call_from(
        &runtime,
        LEAD,
        Request::Wait {
            target: worker.0,
            timeout_secs: 10,
        },
    )?)?;
    assert!(matches!(waited, Response::Settled { .. }), "{waited:?}");
    let order = CloseOrder {
        target: CloseTarget::Agent(worker.0),
        when_working: WhenWorking::Refuse,
        worktree: WorktreeFate::Remove,
    };
    let closed = answered(call_from(&runtime, LEAD, Request::Close(order))?)?;
    assert_eq!(
        closed,
        Response::Closed(vec![Closed {
            target: ClosedTarget::Agent(worker.0),
            worktree: Some(WorktreeLeft {
                path: group.root.clone(),
                outcome: WorktreeOutcome::Removed,
            }),
        }])
    );
    let ended = main_environment(&runtime)?;
    assert!(ended.group(group.id).is_none());
    assert!(!group.root.exists());
    Ok(())
}
