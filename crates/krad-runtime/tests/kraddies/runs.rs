use super::repo::committed_repo;
use super::{
    claude_at, environment_at, environment_of, harness, kraddies_of, start, wait_for, Folder,
    TestClock, TestResult, BULLETS_HARNESS, ENVIRONMENT,
};
use futures::executor::block_on;
use krad_domain::{
    AcpAgent, AgentPreset, Automation, AutomationId, AutomationKey, EnvironmentName,
    IntervalMinutes, Kraddie, KraddieOutcome, KraddieSchedule, KraddieWake, KraddieWorkspace,
    Settings, Trigger,
};
use krad_protocol::{Command, ErrorCode, KraddieCommand, Reply};
use krad_runtime::KraddiesSetup;
use krad_store::KraddieShelf;
use std::error::Error;
use std::path::Path;
use std::time::Duration;

const FEW_TICKS: Duration = Duration::from_millis(300);
const HOURLY: u32 = 60;
const PAST_THE_HOUR: Duration = Duration::from_mins(61);
const BRANCH_PREFIX: &str = "kraddie-night-owl";
const ACP_AGENT: &str = "tests/fixtures/fake-harness/acp-agent";
const ACP_ID: &str = "fake";
const CONFIG_HOME_VAR: &str = "XDG_CONFIG_HOME";

fn shelved(shelf: &KraddieShelf, repo: &Path, trigger: Trigger) -> Result<Kraddie, Box<dyn Error>> {
    shelved_as(shelf, (repo, AgentPreset::ClaudeCode), trigger)
}

fn shelved_as(
    shelf: &KraddieShelf,
    (repo, harness): (&Path, AgentPreset),
    trigger: Trigger,
) -> Result<Kraddie, Box<dyn Error>> {
    let mut kraddie = Kraddie::new("Night Owl".parse()?, harness);
    kraddie.workspace = Some(KraddieWorkspace {
        environment: EnvironmentName::new(ENVIRONMENT.to_owned()),
        repo: repo.to_path_buf(),
    });
    let mut automation = Automation::new(AutomationId(1), "Sweep".parse()?);
    automation.triggers = vec![trigger];
    shelf.write(&kraddie)?;
    shelf.write_automations(&kraddie.id(), &[automation])?;
    Ok(kraddie)
}

fn hourly() -> Result<Trigger, Box<dyn Error>> {
    Ok(Trigger::Schedule {
        schedule: KraddieSchedule::Every {
            minutes: IntervalMinutes::try_from(HOURLY).map_err(|_| "not an interval")?,
        },
    })
}

#[test]
fn a_scheduled_automation_fires_once_in_its_own_worktree_without_a_window() -> TestResult {
    let repo = committed_repo("kraddie-scheduled")?;
    let folder = Folder::new("kraddie-shelf")?;
    let shelf = KraddieShelf::at(folder.0.clone());
    shelved(&shelf, &repo.0, hourly()?)?;
    let clock = TestClock::new();
    let setup = KraddiesSetup {
        shelf: Some(shelf.clone()),
        ordinary: None,
        clock: clock.clock(),
    };
    let data = environment_at(&repo.0, &[])?;
    let runtime = start(data, claude_at(&harness(BULLETS_HARNESS)?), (setup, None))?;
    std::thread::sleep(FEW_TICKS);
    let idle = kraddies_of(&runtime)?;
    let [kraddie] = idle.kraddies.as_slice() else {
        return Err("the runtime did not load the Kraddie".into());
    };
    assert!(kraddie.automations[0].history.is_empty());
    clock.advance(PAST_THE_HOUR);
    let run = wait_for(|| {
        let snapshot = kraddies_of(&runtime)?;
        Ok(snapshot
            .kraddies
            .first()
            .and_then(|kraddie| kraddie.automations.first())
            .and_then(|routine| routine.history.latest())
            .filter(|run| run.outcome == KraddieOutcome::Finished)
            .cloned())
    })?;
    assert_eq!(run.wake, KraddieWake::Schedule);
    let link = run.session.ok_or("the run kept no session")?;
    let data = environment_of(&runtime)?;
    let group = data
        .group_of(link.terminal)
        .ok_or("the run's chat has no group")?;
    let origin = group.worktree.as_ref().ok_or("the run had no worktree")?;
    assert!(origin.branch.starts_with(BRANCH_PREFIX));
    assert_eq!(origin.repo_root, repo.0);
    assert!(group.root.exists());
    std::thread::sleep(FEW_TICKS);
    let after = kraddies_of(&runtime)?;
    assert_eq!(after.kraddies[0].automations[0].history.len(), 1);
    assert!(after.kraddies[0]
        .activity
        .days()
        .any(|(_, day)| day.runs == 1 && day.routines == 1));
    block_on(runtime.settle())?;
    let read = shelf.read_all();
    let saved = read[0].automations[0]
        .history
        .latest()
        .ok_or("no run saved")?;
    assert_eq!(saved.outcome, KraddieOutcome::Finished);
    Ok(())
}

#[test]
fn a_second_run_while_the_first_is_live_is_refused_and_noted_once() -> TestResult {
    let repo = committed_repo("kraddie-manual")?;
    let folder = Folder::new("kraddie-manual-shelf")?;
    let shelf = KraddieShelf::at(folder.0.clone());
    let kraddie = shelved(&shelf, &repo.0, Trigger::Manual)?;
    let setup = KraddiesSetup {
        shelf: Some(shelf),
        ordinary: None,
        clock: TestClock::new().clock(),
    };
    let data = environment_at(&repo.0, &[])?;
    let runtime = start(data, claude_at(&harness(BULLETS_HARNESS)?), (setup, None))?;
    let run = || {
        Command::Kraddie(KraddieCommand::Run {
            automation: AutomationKey::new(kraddie.id(), AutomationId(1)),
            prompt: None,
        })
    };
    assert_eq!(
        block_on(runtime.send(runtime.next_request(), run()))?,
        Reply::Accepted
    );
    for _ in 0..2 {
        let refused = block_on(runtime.send(runtime.next_request(), run()))?;
        assert!(matches!(
            refused,
            Reply::Refused {
                code: ErrorCode::Unavailable,
                ..
            }
        ));
    }
    let history = kraddies_of(&runtime)?.kraddies[0].automations[0]
        .history
        .clone();
    let outcomes: Vec<bool> = history
        .newest_first()
        .map(|run| matches!(run.outcome, KraddieOutcome::Refused { .. }))
        .collect();
    assert_eq!(outcomes, [true, false]);
    Ok(())
}

#[test]
fn an_acp_kraddie_runs_in_the_runtime() -> TestResult {
    let repo = committed_repo("kraddie-acp")?;
    let folder = Folder::new("kraddie-acp-shelf")?;
    std::env::set_var(CONFIG_HOME_VAR, folder.0.join("config"));
    let mut settings = Settings::default();
    let agent = AcpAgent {
        id: ACP_ID.parse().map_err(|_| "the fake agent id is valid")?,
        name: "Fake".to_owned(),
        program: harness(ACP_AGENT)?,
        args: Vec::new(),
        env: Vec::new(),
        terminal: None,
    };
    let preset = AgentPreset::Acp(agent.id);
    settings.acp.add_agent(agent)?;
    let shelf = KraddieShelf::at(folder.0.join("shelf"));
    let kraddie = shelved_as(&shelf, (&repo.0, preset), Trigger::Manual)?;
    let setup = KraddiesSetup {
        shelf: Some(shelf),
        ordinary: None,
        clock: TestClock::new().clock(),
    };
    let runtime = start(environment_at(&repo.0, &[])?, settings, (setup, None))?;
    let run = Command::Kraddie(KraddieCommand::Run {
        automation: AutomationKey::new(kraddie.id(), AutomationId(1)),
        prompt: None,
    });
    assert_eq!(
        block_on(runtime.send(runtime.next_request(), run))?,
        Reply::Accepted
    );
    let finished = wait_for(|| {
        Ok(kraddies_of(&runtime)?
            .kraddies
            .first()
            .and_then(|kraddie| kraddie.automations.first())
            .and_then(|routine| routine.history.latest())
            .filter(|run| run.outcome == KraddieOutcome::Finished)
            .cloned())
    })?;
    let link = finished.session.ok_or("the run kept no session")?;
    let node = environment_of(&runtime)?
        .nodes
        .into_iter()
        .find(|node| node.id == link.terminal)
        .ok_or("the run's chat has no node")?;
    assert_eq!(node.harness, Some(preset));
    Ok(())
}
