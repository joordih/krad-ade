use futures::executor::block_on;
use futures::StreamExt as _;
use krad_domain::{AgentPreset, HarnessHealth, HarnessVersion, SignIn};
use krad_protocol::{EventKind, Feed, HarnessCheck, HarnessChecked, HealthPlace, Scope};
use krad_runtime::{EventReceiver, HarnessProbe, Runtime, RuntimeSetup};
use std::error::Error;

type TestResult = Result<(), Box<dyn Error>>;

const FEED_ENDED: &str = "the feed ended";

fn subscribed(events: &mut EventReceiver) -> TestResult {
    match block_on(events.next()).ok_or(FEED_ENDED)? {
        Feed::Snapshot(_) => Ok(()),
        Feed::Event(_) => Err("the feed did not start from a snapshot".into()),
    }
}

fn next_check(events: &mut EventReceiver) -> Result<HarnessChecked, Box<dyn Error>> {
    loop {
        if let Feed::Event(event) = block_on(events.next()).ok_or(FEED_ENDED)? {
            if let EventKind::HarnessChecked(checked) = event.kind {
                return Ok(checked);
            }
        }
    }
}

fn checked(place: &HealthPlace, harness: AgentPreset, check: HarnessCheck) -> HarnessChecked {
    HarnessChecked {
        place: place.clone(),
        harness,
        check,
    }
}

#[test]
fn a_probe_is_reported_checking_then_done_and_a_vanished_runner_forgets_it() -> TestResult {
    let runtime = Runtime::start(RuntimeSetup::default())?;
    let mut events = runtime.subscribe();
    subscribed(&mut events)?;
    let place = HealthPlace::ThisComputer;
    runtime.check_harnesses(
        place.clone(),
        vec![(AgentPreset::Opencode, Some(HarnessProbe::Missing))],
    );
    let opencode = |check| checked(&place, AgentPreset::Opencode, check);
    assert_eq!(next_check(&mut events)?, opencode(HarnessCheck::Checking));
    let done = opencode(HarnessCheck::Done(HarnessHealth::Missing));
    assert_eq!(next_check(&mut events)?, done);
    let snapshot = block_on(runtime.snapshot(Scope::Runtime))?;
    assert_eq!(snapshot.health.checks, vec![done]);
    runtime.check_harnesses(place.clone(), vec![(AgentPreset::Opencode, None)]);
    assert_eq!(next_check(&mut events)?, opencode(HarnessCheck::Forgotten));
    let snapshot = block_on(runtime.snapshot(Scope::Runtime))?;
    assert_eq!(snapshot.health.checks, Vec::new());
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_fake_codex_reports_its_version_and_sign_in() -> TestResult {
    use krad_chat::Runner;
    use std::path::{Path, PathBuf};

    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-harness");
    let runtime = Runtime::start(RuntimeSetup::default())?;
    let mut events = runtime.subscribe();
    subscribed(&mut events)?;
    let place = HealthPlace::ThisComputer;
    let probe = HarnessProbe::Run(Runner::Native(fixtures.join("codex")), PathBuf::from("/"));
    runtime.check_harnesses(place.clone(), vec![(AgentPreset::Codex, Some(probe))]);
    assert_eq!(
        next_check(&mut events)?.check,
        HarnessCheck::Checking,
        "the check starts before the probe answers"
    );
    assert_eq!(
        next_check(&mut events)?,
        checked(
            &place,
            AgentPreset::Codex,
            HarnessCheck::Done(HarnessHealth::Found {
                version: Some(HarnessVersion::new(9, 1, 0)),
                sign_in: SignIn::SignedIn,
            }),
        )
    );
    Ok(())
}
