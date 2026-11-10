use super::*;

const NOW: u64 = 1_000_000;
const MINUTE: u64 = 60;
const HOUR: u64 = 60 * MINUTE;
const DAY: u64 = 24 * HOUR;

fn now() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(NOW)
}

fn warn_at() -> WarnPercent {
    WarnPercent::try_from(80).unwrap_or_else(|_| unreachable!("80 is a valid share"))
}

fn window(span: WindowSpan, used: u16, resets_in: Option<u64>) -> UsageWindow {
    UsageWindow {
        span,
        scope: None,
        used,
        resets_at: resets_in.map(|secs| ResetTime::from_epoch_secs(NOW + secs)),
        limited: false,
    }
}

fn claude_usage() -> PlanUsage {
    let mut opus = window(WindowSpan::Weekly, 90, Some(4 * DAY));
    opus.scope = Some("Opus".to_owned());
    PlanUsage {
        plan: Some("max".to_owned()),
        windows: vec![
            window(
                WindowSpan::Weekly,
                41,
                Some(4 * DAY + 6 * HOUR + 10 * MINUTE),
            ),
            opus,
            window(WindowSpan::FiveHours, 24, Some(2 * HOUR + 48 * MINUTE + 30)),
        ],
        alert: None,
    }
}

fn block(reading: Option<&UsageReading>) -> UsageBlock {
    usage_block(
        BlockInput {
            harness: AgentPreset::ClaudeCode,
            reading,
            note: None,
        },
        warn_at(),
        now(),
    )
}

#[test]
fn time_to_reset_reads_in_days_and_hours_or_hours_and_minutes() {
    assert_eq!(
        left_text(Duration::from_secs(2 * HOUR + 48 * MINUTE + 59)),
        "2h 48m"
    );
    assert_eq!(
        left_text(Duration::from_secs(4 * DAY + 6 * HOUR + 59 * MINUTE)),
        "4d 6h"
    );
    assert_eq!(left_text(Duration::from_secs(35 * MINUTE)), "35m");
    assert_eq!(left_text(Duration::from_secs(3 * HOUR)), "3h 0m");
    assert_eq!(left_text(Duration::from_secs(20)), "<1m");
}

#[test]
fn a_window_cell_shows_its_percent_and_its_reset_and_warns_from_the_chosen_share() {
    let cell = window_cell(
        &window(WindowSpan::FiveHours, 24, Some(2 * HOUR + 48 * MINUTE)),
        warn_at(),
        now(),
    );
    assert_eq!(
        cell,
        Cell {
            text: "24% 2h 48m".to_owned(),
            tone: Tone::Calm
        }
    );
    let near = window_cell(&window(WindowSpan::Weekly, 85, None), warn_at(), now());
    assert_eq!(
        near,
        Cell {
            text: "85%".to_owned(),
            tone: Tone::Warn
        }
    );
    let mut limited = window(WindowSpan::Rolling, 30, Some(0));
    limited.limited = true;
    assert_eq!(
        window_cell(&limited, warn_at(), now()),
        Cell {
            text: "30%".to_owned(),
            tone: Tone::Limit
        }
    );
}

#[test]
fn the_bar_meters_the_five_hour_window_and_lists_the_unscoped_windows() {
    let reading = UsageReading::Fresh(claude_usage());
    let shown = block(Some(&reading));
    assert_eq!(
        shown.meter,
        Meter {
            used: 24,
            tone: Tone::Calm
        }
    );
    let cells: Vec<&str> = shown.cells.iter().map(|cell| cell.text.as_str()).collect();
    assert_eq!(cells, ["24% 2h 48m", "41% 4d 6h"]);
    assert!(!shown.stale);
    assert_eq!(shown.status, None);
    assert_eq!(
        shown.tooltip,
        [
            "Claude Code · Plan: max",
            "5 hours 24% · resets in 2h 48m",
            "Weekly 41% · resets in 4d 6h",
            "Weekly (Opus) 90% · resets in 4d 0h",
        ]
        .join("\n")
    );
}

#[test]
fn without_a_five_hour_window_the_bar_meters_the_weekly_one() {
    let usage = PlanUsage {
        windows: vec![window(WindowSpan::Weekly, 60, None)],
        ..PlanUsage::default()
    };
    let shown = block(Some(&UsageReading::Fresh(usage)));
    assert_eq!(shown.meter.used, 60);
}

#[test]
fn a_failed_read_keeps_showing_the_last_usage_marked_stale() {
    let reading = UsageReading::Failed {
        last: Some(claude_usage()),
        reason: UsageFailure::Unreachable,
    };
    let shown = block(Some(&reading));
    assert!(shown.stale);
    assert_eq!(shown.cells.len(), 2);
    assert!(shown
        .tooltip
        .ends_with("tries again in a while.\nShowing the last reading."));
}

#[test]
fn without_any_usage_the_block_says_why_in_a_word_and_explains_on_hover() {
    let status = |reason| {
        let reading = UsageReading::Failed { last: None, reason };
        let shown = block(Some(&reading));
        (
            shown.status,
            shown.meter == EMPTY_METER && shown.cells.is_empty() && shown.stale,
        )
    };
    assert_eq!(status(UsageFailure::SignedOut), (Some("Signed out"), true));
    assert_eq!(
        status(UsageFailure::NoSubscription),
        (Some("No plan"), true)
    );
    assert_eq!(
        status(UsageFailure::RateLimited { retry_at: None }),
        (Some("Rate limited"), true)
    );
    assert_eq!(
        status(UsageFailure::Unreachable),
        (Some("Not available"), true)
    );
    let limited = UsageReading::Failed {
        last: None,
        reason: UsageFailure::RateLimited {
            retry_at: Some(ResetTime::from_epoch_secs(NOW + 20 * MINUTE)),
        },
    };
    assert!(block(Some(&limited))
        .tooltip
        .ends_with("asks again in 20m."));
    assert_eq!(block(None).status, Some("Reading…"));
}

fn opencode_block(reading: Option<&UsageReading>, note: &UsageNote) -> UsageBlock {
    usage_block(
        BlockInput {
            harness: AgentPreset::Opencode,
            reading,
            note: Some(note),
        },
        warn_at(),
        now(),
    )
}

#[test]
fn a_server_that_is_starting_or_did_not_start_says_so_instead_of_reading_forever() {
    let starting = opencode_block(None, &UsageNote::Starting);
    assert_eq!(starting.status, Some("Starting…"));
    assert!(starting
        .tooltip
        .starts_with("opencode\nWaiting for the harness's server"));
    let remote = opencode_block(None, &UsageNote::HostNotReady);
    assert_eq!(
        (remote.status, remote.tooltip.as_str()),
        (
            Some("Not available"),
            "opencode\nUsage cannot be read on this host yet."
        )
    );
    let broken = UsageNote::Broken("The opencode server did not start: port in use".to_owned());
    let failed = UsageReading::Failed {
        last: None,
        reason: UsageFailure::Unreachable,
    };
    let shown = opencode_block(Some(&failed), &broken);
    assert_eq!(
        (shown.status, shown.tooltip.as_str()),
        (
            Some("Not available"),
            "opencode\nThe opencode server did not start: port in use"
        )
    );
    let kept = UsageReading::Failed {
        last: Some(claude_usage()),
        reason: UsageFailure::Unreachable,
    };
    let stale = opencode_block(Some(&kept), &broken);
    assert!(stale.stale);
    assert!(stale
        .tooltip
        .ends_with("Showing the last reading.\nThe opencode server did not start: port in use"));
}

#[test]
fn a_reached_limit_turns_the_meter_to_the_limit_tone() {
    let mut usage = claude_usage();
    usage.alert = Some(LimitAlert {
        status: LimitStatus::Reached,
        resets_at: None,
        heard_at: now(),
    });
    let shown = block(Some(&UsageReading::Fresh(usage)));
    assert_eq!(shown.meter.tone, Tone::Limit);
    assert!(shown.tooltip.contains("\nLimit reached\n"));
}

#[test]
fn a_limit_alert_whose_reset_has_passed_is_no_longer_shown() {
    let mut usage = claude_usage();
    usage.alert = Some(LimitAlert {
        status: LimitStatus::Reached,
        resets_at: Some(ResetTime::from_epoch_secs(NOW - MINUTE)),
        heard_at: now() - Duration::from_secs(2 * MINUTE),
    });
    let shown = block(Some(&UsageReading::Fresh(usage)));
    assert_ne!(shown.meter.tone, Tone::Limit);
    assert!(!shown.tooltip.contains("Limit reached"));
}
