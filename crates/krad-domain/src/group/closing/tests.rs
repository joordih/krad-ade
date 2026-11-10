use super::*;
use crate::environment::seed_environment;
use crate::group::{SessionLead, SessionSpec, WorkerSpawn, WorktreeOrigin};
use crate::persisted_layout::{PersistedLayout, LAYOUT_SCHEMA_VERSION};
use crate::sidebar_width::SidebarWidth;
use crate::toolchain::ToolchainChoices;
use crate::work_mode::WorkMode;
use crate::AgentPreset;
use std::path::PathBuf;

const LEAD_TAB: TerminalId = TerminalId(1);
const PLAIN_TAB: TerminalId = TerminalId(3);
const WORKTREE_WORKER: TerminalId = TerminalId(4);
const WORKER: TerminalId = TerminalId(5);
const OTHERS_WORKER: TerminalId = TerminalId(6);
const MISSING: u64 = 99;

struct Fixture {
    environment: EnvironmentData,
    lead: SessionId,
    other_lead: SessionId,
    plain: SessionId,
    fix: GroupId,
}

fn worker_of(lead: SessionId) -> WorkerSpawn {
    WorkerSpawn {
        lead: Some(lead),
        task: "Fix the webhook retries".to_owned(),
    }
}

fn lead_session(environment: &mut EnvironmentData, id: SessionId, tab: Option<TerminalId>) {
    if let Some(session) = environment.sessions.iter_mut().find(|s| s.id == id) {
        let mut lead = SessionLead::new(AgentPreset::ClaudeCode, None);
        lead.terminal = tab;
        session.lead = Some(lead);
    }
}

fn fixture() -> Fixture {
    let mut environment = seed_environment();
    let api = environment.add_group(PathBuf::from("/work/api"), None);
    let origin = WorktreeOrigin {
        repo_root: PathBuf::from("/work/api"),
        branch: "fix".to_owned(),
    };
    let fix = environment.add_group(PathBuf::from("/work/api.worktrees/fix"), Some(origin));
    for node in &mut environment.nodes {
        node.group = Some(api);
    }
    for (id, group) in [(WORKTREE_WORKER, fix), (WORKER, api), (OTHERS_WORKER, api)] {
        let mut node = environment.nodes[2].clone();
        node.id = id;
        node.group = Some(group);
        environment.nodes.push(node);
    }
    environment.enter_sessions();
    let lead = environment
        .active_session
        .unwrap_or_else(|| unreachable!("Sessions mode opens a session"));
    lead_session(&mut environment, lead, Some(LEAD_TAB));
    let other_lead = environment.new_session(Some(api), SessionSpec::default());
    lead_session(&mut environment, other_lead, None);
    let plain = environment.new_session(Some(api), SessionSpec::default());
    for session in &mut environment.sessions {
        session.tabs.remove_terminal(PLAIN_TAB);
    }
    if let Some(session) = environment.sessions.iter_mut().find(|s| s.id == plain) {
        session.tabs.add_terminal(PLAIN_TAB);
    }
    environment.active_session = Some(lead);
    environment.open_worker_session(WORKTREE_WORKER, worker_of(lead));
    environment.open_worker_session(WORKER, worker_of(lead));
    environment.open_worker_session(OTHERS_WORKER, worker_of(other_lead));
    Fixture {
        environment,
        lead,
        other_lead,
        plain,
        fix,
    }
}

fn settled(_: TerminalId) -> bool {
    false
}

fn plan(fixture: &Fixture, subject: CloseSubject) -> Result<ClosePlan, CloseRefusal> {
    fixture
        .environment
        .plan_close(fixture.lead, subject, WhenWorking::Refuse, settled)
}

fn session_of(fixture: &Fixture, terminal: TerminalId) -> SessionId {
    fixture
        .environment
        .session_of(terminal)
        .unwrap_or_else(|| unreachable!("terminal {terminal:?} has a session"))
}

#[test]
fn a_lead_closes_its_own_workers_and_learns_their_worktree() {
    let fixture = fixture();
    assert_eq!(
        plan(&fixture, CloseSubject::Worker(WORKER)),
        Ok(ClosePlan {
            subject: CloseSubject::Worker(WORKER),
            terminals: vec![WORKER],
            cancel: Vec::new(),
            worktree: None,
        })
    );
    let in_worktree = plan(&fixture, CloseSubject::Worker(WORKTREE_WORKER));
    assert_eq!(in_worktree.map(|plan| plan.worktree), Ok(Some(fixture.fix)));
}

#[test]
fn unknown_foreign_and_own_targets_are_refused_apart() {
    let fixture = fixture();
    let cases = [
        (
            CloseSubject::Worker(TerminalId(MISSING)),
            CloseRefusal::Unknown,
        ),
        (CloseSubject::Worker(OTHERS_WORKER), CloseRefusal::NotOwned),
        (CloseSubject::Worker(TerminalId(2)), CloseRefusal::NotOwned),
        (CloseSubject::Worker(LEAD_TAB), CloseRefusal::Itself),
        (CloseSubject::Session(fixture.lead), CloseRefusal::Itself),
        (
            CloseSubject::Session(SessionId(MISSING)),
            CloseRefusal::Unknown,
        ),
        (
            CloseSubject::Session(fixture.other_lead),
            CloseRefusal::NotOwned,
        ),
        (
            CloseSubject::Session(session_of(&fixture, OTHERS_WORKER)),
            CloseRefusal::NotOwned,
        ),
    ];
    for (subject, refusal) in cases {
        assert_eq!(plan(&fixture, subject), Err(refusal), "{subject:?}");
    }
}

#[test]
fn sessions_of_the_leads_group_and_of_its_workers_can_close() {
    let mut fixture = fixture();
    let plain = plan(&fixture, CloseSubject::Session(fixture.plain));
    assert_eq!(plain.map(|plan| plan.terminals), Ok(vec![PLAIN_TAB]));
    let worktree_session = session_of(&fixture, WORKTREE_WORKER);
    let held = plan(&fixture, CloseSubject::Session(worktree_session));
    assert_eq!(
        held.map(|plan| (plan.terminals, plan.worktree)),
        Ok((vec![WORKTREE_WORKER], Some(fixture.fix)))
    );
    let web = fixture
        .environment
        .add_group(PathBuf::from("/work/web"), None);
    let elsewhere = fixture
        .environment
        .new_session(Some(web), SessionSpec::default());
    assert_eq!(
        plan(&fixture, CloseSubject::Session(elsewhere)),
        Err(CloseRefusal::NotOwned)
    );
}

#[test]
fn a_working_target_is_refused_unless_its_turn_is_cancelled() {
    let fixture = fixture();
    let busy = |terminal: TerminalId| terminal == WORKER;
    let subject = CloseSubject::Worker(WORKER);
    assert_eq!(
        fixture
            .environment
            .plan_close(fixture.lead, subject, WhenWorking::Refuse, busy),
        Err(CloseRefusal::Working)
    );
    let forced =
        fixture
            .environment
            .plan_close(fixture.lead, subject, WhenWorking::CancelTurn, busy);
    assert_eq!(forced.map(|plan| plan.cancel), Ok(vec![WORKER]));
    assert_eq!(
        fixture.environment.idle_workers(fixture.lead, busy),
        vec![WORKTREE_WORKER]
    );
    assert_eq!(
        fixture
            .environment
            .idle_workers(fixture.other_lead, settled),
        vec![OTHERS_WORKER]
    );
}

#[test]
fn a_closed_worker_leaves_the_persisted_layout_and_cannot_close_twice() -> serde_json::Result<()> {
    let mut fixture = fixture();
    let session = session_of(&fixture, WORKER);
    fixture.environment.drop_terminal(WORKER);
    assert_eq!(
        plan(&fixture, CloseSubject::Worker(WORKER)),
        Err(CloseRefusal::AlreadyClosed)
    );
    let layout = PersistedLayout {
        version: LAYOUT_SCHEMA_VERSION,
        active: fixture.environment.name.clone(),
        environments: vec![fixture.environment],
        unreadable: Vec::new(),
        work_mode: WorkMode::default(),
        sidebar_width: SidebarWidth::default(),
        toolchains: ToolchainChoices::default(),
    };
    let back: PersistedLayout = serde_json::from_str(&serde_json::to_string(&layout)?)?;
    let Some(restored) = back.environments.first() else {
        return Err(serde::de::Error::custom("no environment"));
    };
    assert!(restored.session(session).is_none());
    assert!(restored.session_of(WORKER).is_none());
    assert!(restored.nodes.iter().all(|node| node.id != WORKER));
    assert_eq!(restored.open_workers(fixture.lead), vec![WORKTREE_WORKER]);
    assert!(restored
        .worker(WORKER)
        .is_some_and(|(owner, worker)| owner == fixture.lead && worker.session.is_none()));
    Ok(())
}
