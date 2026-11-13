use futures::executor::block_on;
use futures::StreamExt as _;
use krad_domain::{
    AgentPreset, AgentStatus, Host, PlanUsage, ResetTime, Settings, UsageReading, WindowSpan,
};
use krad_protocol::{
    Command, EventKind, Feed, HarnessSpot, Reply, Scope, UsageCommand, UsageNews, UsageNote,
    UsageShown,
};
use krad_runtime::{EventReceiver, Runtime, RuntimeHandle, RuntimeSetup, SettingsSetup};
use std::collections::HashMap;
use std::error::Error;

type TestResult = Result<(), Box<dyn Error>>;

const FEED_ENDED: &str = "the feed ended";
const MISSING_PROGRAM: &str = "krad-runtime-test-no-such-harness";
const NOT_ON_PATH: &str = "is not on PATH";

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
fn a_placed_harness_with_a_reading_but_no_program_shows_why_it_cannot_be_read() -> TestResult {
    let settings = Settings {
        harness_commands: HashMap::from([(AgentPreset::ClaudeCode, MISSING_PROGRAM.to_owned())]),
        ..Settings::default()
    };
    let why = format!(
        "{} {NOT_ON_PATH}",
        settings.acp.harness_name(AgentPreset::ClaudeCode)
    );
    let runtime = Runtime::start(RuntimeSetup {
        settings: SettingsSetup {
            settings,
            file: None,
        },
        ..RuntimeSetup::default()
    })?;
    let mut events = subscribed(&runtime)?;
    let status = AgentStatus {
        five_hour_used: Some(30),
        ..AgentStatus::default()
    };
    send(
        &runtime,
        UsageCommand::Statusline {
            host: Host::Local,
            status,
        },
    )?;
    next_shown(&mut events)?;
    let placed = send(
        &runtime,
        UsageCommand::Place {
            spot: Some(HarnessSpot::default()),
        },
    )?;
    assert_eq!(placed, Reply::Accepted);
    send(&runtime, UsageCommand::Refresh)?;
    let noted = loop {
        let shown = next_shown(&mut events)?;
        if let Some(note) = shown.note {
            break (shown.harness, note);
        }
    };
    assert_eq!(noted, (AgentPreset::ClaudeCode, UsageNote::Broken(why)));
    let snapshot = block_on(runtime.snapshot(Scope::Runtime))?;
    assert_eq!(
        snapshot.usage.shown.len(),
        1,
        "harnesses never read nor installed stay out"
    );
    Ok(())
}
