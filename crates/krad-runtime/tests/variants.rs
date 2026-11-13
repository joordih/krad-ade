#![cfg(unix)]

#[path = "support/repo.rs"]
mod repo;

use futures::executor::block_on;
use futures::StreamExt as _;
use krad_domain::{
    AgentPreset, AppEvent, EnvironmentData, EnvironmentName, RuntimeState, Settings, VariantId,
    VariantRemoval, VariantRun, VariantRunId, VariantSpec, VariantStatus, VariantsOrder,
    WinnerOutcome,
};
use krad_protocol::{
    Command, ErrorCode, EventKind, Feed, Reply, Scope, VariantAct, VariantCommand,
};
use krad_runtime::{EnvironmentsSetup, Runtime, RuntimeHandle, RuntimeSetup, SettingsSetup};
use repo::committed_repo;
use std::error::Error;
use std::path::Path;
use std::time::{Duration, Instant};

type TestResult = Result<(), Box<dyn Error>>;

const ENVIRONMENT: &str = "main";
const FAKE_CLAUDE: &str = "../krad-chat/tests/fixtures/fake-harness/claude";
const WAIT: Duration = Duration::from_secs(20);
const POLL: Duration = Duration::from_millis(50);
const TIMED_OUT: &str = "the awaited state never came";

fn environment() -> EnvironmentName {
    EnvironmentName::new(ENVIRONMENT.to_owned())
}

fn start(root: &Path) -> Result<RuntimeHandle, Box<dyn Error>> {
    let mut data = EnvironmentData::empty(environment());
    data.add_group(root.to_path_buf(), None);
    data.enter_sessions();
    let fake = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(FAKE_CLAUDE)
        .canonicalize()?;
    let mut settings = Settings::default();
    settings
        .harness_commands
        .insert(AgentPreset::ClaudeCode, fake.to_string_lossy().into_owned());
    Ok(Runtime::start(RuntimeSetup {
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

fn send(runtime: &RuntimeHandle, act: VariantAct) -> Result<Reply, Box<dyn Error>> {
    let command = Command::Variant(VariantCommand {
        environment: environment(),
        act,
    });
    Ok(block_on(runtime.send(runtime.next_request(), command))?)
}

fn data_of(runtime: &RuntimeHandle) -> Result<EnvironmentData, Box<dyn Error>> {
    block_on(runtime.snapshot(Scope::Runtime))?
        .environments
        .into_iter()
        .next()
        .ok_or_else(|| "the runtime lost the environment".into())
}

fn wait_for<T>(
    mut found: impl FnMut() -> Result<Option<T>, Box<dyn Error>>,
) -> Result<T, Box<dyn Error>> {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if let Some(value) = found()? {
            return Ok(value);
        }
        std::thread::sleep(POLL);
    }
    Err(TIMED_OUT.into())
}

fn finished_run(runtime: &RuntimeHandle) -> Result<Option<VariantRun>, Box<dyn Error>> {
    let data = data_of(runtime)?;
    Ok(data.variant_runs.all().first().cloned().filter(|run| {
        run.variants
            .iter()
            .all(|variant| variant.status == VariantStatus::Finished && variant.diff.is_some())
    }))
}

#[test]
fn variants_run_in_their_worktrees_and_the_first_pick_wins() -> TestResult {
    let repo = committed_repo("variants")?;
    let runtime = start(&repo.0)?;
    let mut events = runtime.subscribe();
    let group = data_of(&runtime)?.groups[0].id;
    let order = VariantsOrder::new(
        "Fix the race",
        vec![
            VariantSpec::new(AgentPreset::ClaudeCode, None, None),
            VariantSpec::new(AgentPreset::ClaudeCode, None, Some("Keep it small")),
        ],
        None,
    )?;
    wait_for(|| {
        let run = VariantAct::Run {
            group,
            order: Box::new(order.clone()),
        };
        Ok((send(&runtime, run)? == Reply::Accepted).then_some(()))
    })?;
    let run = wait_for(|| finished_run(&runtime))?;
    let data = data_of(&runtime)?;
    for variant in &run.variants {
        let worktree = variant
            .worktree
            .as_ref()
            .ok_or("a variant has no worktree")?;
        assert!(worktree.path.exists());
        assert!(data.group(worktree.group).is_some());
    }
    let loser = run.variants[1]
        .worktree
        .clone()
        .ok_or("the loser has no worktree")?;
    let pick = |winner: u32| VariantAct::Pick {
        run: run.id,
        winner: VariantId(winner),
        outcome: WinnerOutcome::KeepWorktree,
        removal: VariantRemoval::Forced,
    };
    assert_eq!(send(&runtime, pick(1))?, Reply::Accepted);
    let again = send(&runtime, pick(2))?;
    assert!(matches!(
        again,
        Reply::Refused {
            code: ErrorCode::AlreadyAnswered,
            ..
        }
    ));
    let deadline = Instant::now() + WAIT;
    let chosen = loop {
        if Instant::now() > deadline {
            return Err(TIMED_OUT.into());
        }
        if let Some(Feed::Event(event)) = block_on(events.next()) {
            if let EventKind::AppEvent(record) = event.kind {
                if let AppEvent::VariantWinnerChosen {
                    run: picked,
                    variant,
                    ..
                } = record.event
                {
                    break (picked, variant);
                }
            }
        }
    };
    assert_eq!(chosen, (run.id, VariantId(1)));
    let after = data_of(&runtime)?;
    let decided = after
        .variant_runs
        .run(VariantRunId(run.id.0))
        .ok_or("the run is gone")?;
    assert_eq!(decided.variants[0].status, VariantStatus::Chosen);
    assert_eq!(decided.variants[1].status, VariantStatus::Discarded);
    assert!(after.group(loser.group).is_none());
    wait_for(|| Ok((!loser.path.exists()).then_some(())))?;
    Ok(())
}
