use futures::executor::block_on;
use futures::StreamExt as _;
use krad_domain::{AgentPreset, EnvironmentName, HarnessHealth, Settings, CHECKED_HARNESSES};
use krad_protocol::{
    Command, ErrorCode, EventKind, Feed, HarnessCheck, HarnessChecked, HarnessSpot, HealthCommand,
    HealthPlace, Reply, Scope,
};
use krad_runtime::{EventReceiver, Runtime, RuntimeHandle, RuntimeSetup, SettingsSetup};
use std::collections::HashMap;
use std::error::Error;

type TestResult = Result<(), Box<dyn Error>>;

const FEED_ENDED: &str = "the feed ended";
const MISSING_PROGRAM: &str = "krad-runtime-test-no-such-harness";

fn subscribed(events: &mut EventReceiver) -> TestResult {
    match block_on(events.next()).ok_or(FEED_ENDED)? {
        Feed::Snapshot(_) => Ok(()),
        Feed::Event(_) => Err("the feed did not start from a snapshot".into()),
    }
}

fn next_check(
    events: &mut EventReceiver,
    harness: AgentPreset,
) -> Result<HarnessChecked, Box<dyn Error>> {
    loop {
        if let Feed::Event(event) = block_on(events.next()).ok_or(FEED_ENDED)? {
            match event.kind {
                EventKind::HarnessChecked(checked) if checked.harness == harness => {
                    return Ok(checked);
                }
                _ => {}
            }
        }
    }
}

fn started(commands: HashMap<AgentPreset, String>) -> std::io::Result<RuntimeHandle> {
    Runtime::start(RuntimeSetup {
        settings: SettingsSetup {
            settings: Settings {
                harness_commands: commands,
                ..Settings::default()
            },
            file: None,
        },
        ..RuntimeSetup::default()
    })
}

fn missing_everywhere() -> HashMap<AgentPreset, String> {
    CHECKED_HARNESSES
        .into_iter()
        .map(|harness| (harness, MISSING_PROGRAM.to_owned()))
        .collect()
}

fn check(runtime: &RuntimeHandle, spot: HarnessSpot) -> Result<Reply, Box<dyn Error>> {
    Ok(block_on(runtime.send(
        runtime.next_request(),
        Command::Health(HealthCommand::Check { spot }),
    ))?)
}

#[test]
fn a_check_on_this_computer_reports_each_harness_checking_then_missing() -> TestResult {
    let runtime = started(missing_everywhere())?;
    let mut events = runtime.subscribe();
    subscribed(&mut events)?;
    assert_eq!(check(&runtime, HarnessSpot::default())?, Reply::Accepted);
    let mut heard: Vec<HarnessChecked> = Vec::new();
    while heard.len() < 2 * CHECKED_HARNESSES.len() {
        if let Feed::Event(event) = block_on(events.next()).ok_or(FEED_ENDED)? {
            if let EventKind::HarnessChecked(checked) = event.kind {
                heard.push(checked);
            }
        }
    }
    for harness in CHECKED_HARNESSES {
        let checks: Vec<(HealthPlace, HarnessCheck)> = heard
            .iter()
            .filter(|checked| checked.harness == harness)
            .map(|checked| (checked.place.clone(), checked.check.clone()))
            .collect();
        assert_eq!(
            checks,
            vec![
                (HealthPlace::ThisComputer, HarnessCheck::Checking),
                (
                    HealthPlace::ThisComputer,
                    HarnessCheck::Done(HarnessHealth::Missing)
                ),
            ],
            "{harness:?}"
        );
    }
    let snapshot = block_on(runtime.snapshot(Scope::Runtime))?;
    assert_eq!(snapshot.health.checks.len(), CHECKED_HARNESSES.len());
    Ok(())
}

#[test]
fn a_check_in_an_unknown_environment_is_refused() -> TestResult {
    let runtime = started(missing_everywhere())?;
    let spot = HarnessSpot::at(EnvironmentName::new("nowhere".to_owned()), None);
    assert!(matches!(
        check(&runtime, spot)?,
        Reply::Refused {
            code: ErrorCode::NotFound,
            ..
        }
    ));
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_fake_codex_reports_its_version_and_sign_in() -> TestResult {
    use krad_domain::{HarnessVersion, SignIn};
    use std::path::Path;

    let codex = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-harness/codex");
    let mut commands = missing_everywhere();
    commands.insert(AgentPreset::Codex, codex.display().to_string());
    let runtime = started(commands)?;
    let mut events = runtime.subscribe();
    subscribed(&mut events)?;
    check(&runtime, HarnessSpot::default())?;
    assert_eq!(
        next_check(&mut events, AgentPreset::Codex)?.check,
        HarnessCheck::Checking,
        "the check starts before the probe answers"
    );
    assert_eq!(
        next_check(&mut events, AgentPreset::Codex)?.check,
        HarnessCheck::Done(HarnessHealth::Found {
            version: Some(HarnessVersion::new(9, 1, 0)),
            sign_in: SignIn::SignedIn,
        })
    );
    Ok(())
}
