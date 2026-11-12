use futures::executor::block_on;
use futures::StreamExt as _;
use krad_domain::{
    AgentPreset, AgentStatus, Host, PlanUsage, ResetTime, UsageFailure, UsageReading, WindowSpan,
};
use krad_protocol::{
    Command, EventKind, Feed, Reply, Scope, UsageCommand, UsageNews, UsageNote, UsageShown,
};
use krad_runtime::{EventReceiver, Runtime, RuntimeHandle, RuntimeSetup, SourceState, UsagePlace};
use std::error::Error;

type TestResult = Result<(), Box<dyn Error>>;

const FEED_ENDED: &str = "the feed ended";
const NO_RUNNER: &str = "no runner";

fn subscribed(runtime: &RuntimeHandle) -> Result<EventReceiver, Box<dyn Error>> {
    let mut events = runtime.subscribe();
    match block_on(events.next()).ok_or(FEED_ENDED)? {
        Feed::Snapshot(_) => Ok(events),
        Feed::Event(_) => Err("the feed did not start from a snapshot".into()),
    }
}

fn next_shown(events: &mut EventReceiver) -> Result<UsageShown, Box<dyn Error>> {
    loop {
        if let Feed::Event(event) = block_on(events.next()).ok_or(FEED_ENDED)? {
            if let EventKind::UsageShown(shown) = event.kind {
                return Ok(shown);
            }
        }
    }
}

fn send(runtime: &RuntimeHandle, command: UsageCommand) -> Result<Reply, Box<dyn Error>> {
    Ok(block_on(
        runtime.send(runtime.next_request(), Command::Usage(command)),
    )?)
}

fn used(shown: &UsageShown, span: &WindowSpan) -> Option<u16> {
    shown
        .reading
        .as_ref()
        .and_then(UsageReading::usage)
        .and_then(|usage| usage.window(span))
        .map(|window| window.used)
}

#[test]
fn a_statusline_report_becomes_a_claude_reading_kept_in_the_snapshot() -> TestResult {
    let runtime = Runtime::start(RuntimeSetup::default())?;
    let mut events = subscribed(&runtime)?;
    let status = AgentStatus {
        five_hour_used: Some(30),
        five_hour_resets_at: Some(ResetTime::from_epoch_secs(100)),
        seven_day_used: Some(12),
        ..AgentStatus::default()
    };
    let reply = send(
        &runtime,
        UsageCommand::Statusline {
            host: Host::Local,
            status,
        },
    )?;
    assert_eq!(reply, Reply::Accepted);
    let shown = next_shown(&mut events)?;
    assert_eq!(
        (shown.host.clone(), shown.harness),
        (Host::Local, AgentPreset::ClaudeCode)
    );
    assert_eq!(used(&shown, &WindowSpan::FiveHours), Some(30));
    assert_eq!(used(&shown, &WindowSpan::Weekly), Some(12));
    let snapshot = block_on(runtime.snapshot(Scope::Runtime))?;
    assert_eq!(snapshot.usage.shown, vec![shown]);
    Ok(())
}

#[test]
fn an_alert_with_nothing_to_alert_is_dropped_and_a_chat_read_is_shown() -> TestResult {
    let runtime = Runtime::start(RuntimeSetup::default())?;
    let mut events = subscribed(&runtime)?;
    let heard = |news| UsageCommand::Heard {
        host: Host::Local,
        harness: AgentPreset::Codex,
        news,
    };
    send(&runtime, heard(UsageNews::Alert { alert: None }))?;
    let usage = PlanUsage {
        plan: Some("pro".to_owned()),
        ..PlanUsage::default()
    };
    send(
        &runtime,
        heard(UsageNews::Read {
            usage: usage.clone(),
        }),
    )?;
    let shown = next_shown(&mut events)?;
    assert_eq!(shown.reading, Some(UsageReading::Fresh(usage)));
    Ok(())
}

#[test]
fn a_harness_with_no_way_to_be_read_shows_why_and_a_waiting_one_its_note() -> TestResult {
    let runtime = Runtime::start(RuntimeSetup::default())?;
    let mut events = subscribed(&runtime)?;
    runtime.place_usage(Some(UsagePlace {
        host: Host::Local,
        sources: vec![
            (
                AgentPreset::Codex,
                SourceState::Broken(NO_RUNNER.to_owned()),
            ),
            (
                AgentPreset::Opencode,
                SourceState::Waiting(UsageNote::Starting),
            ),
        ],
    }));
    let noted = next_shown(&mut events)?;
    assert_eq!(noted.note, Some(UsageNote::Broken(NO_RUNNER.to_owned())));
    let failed = next_shown(&mut events)?;
    assert_eq!(
        failed.reading.and_then(|reading| reading.failure()),
        Some(UsageFailure::Unreachable)
    );
    let waiting = next_shown(&mut events)?;
    assert_eq!(
        (waiting.harness, waiting.note),
        (AgentPreset::Opencode, Some(UsageNote::Starting))
    );
    let reply = send(&runtime, UsageCommand::Refresh)?;
    assert_eq!(reply, Reply::Accepted);
    let snapshot = block_on(runtime.snapshot(Scope::Runtime))?;
    assert_eq!(snapshot.usage.shown.len(), 2);
    Ok(())
}
