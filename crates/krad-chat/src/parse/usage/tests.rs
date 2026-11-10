use super::*;
use serde_json::json;

fn reply() -> Value {
    json!({
        "subscription_type": "max",
        "rate_limits_available": true,
        "session": {"total_cost_usd": 0},
        "rate_limits": {
            "five_hour": {"utilization": 23.6, "resets_at": "2026-11-09T18:30:00+00:00",
                          "limit_dollars": null, "locked_reason": null},
            "seven_day": {"utilization": 41, "resets_at": 1_794_800_000},
            "seven_day_oauth_apps": null,
            "seven_day_opus": {"utilization": null, "resets_at": null},
            "seven_day_sonnet": {"utilization": 12, "resets_at": 1_794_800_000_000_u64},
            "model_scoped": [
                {"display_name": "Fable", "utilization": 70, "resets_at": "2026-11-15T05:00:00+00:00"},
                {"display_name": "Haiku", "utilization": null, "resets_at": null}
            ],
            "extra_usage": {"is_enabled": false, "utilization": null},
            "limits": [{"kind": "session", "percent": 99, "resets_at": null}]
        }
    })
}

#[test]
fn the_named_windows_and_model_scoped_ones_are_read_with_their_resets() {
    let usage = claude_plan_usage(&reply()).unwrap_or_else(|failure| unreachable!("{failure:?}"));
    assert_eq!(usage.plan.as_deref(), Some("max"));
    assert_eq!(
        usage.window(&WindowSpan::FiveHours),
        Some(&UsageWindow {
            span: WindowSpan::FiveHours,
            scope: None,
            used: 24,
            resets_at: ResetTime::parse_iso("2026-11-09T18:30:00Z"),
            limited: false,
        })
    );
    let weekly = usage.window(&WindowSpan::Weekly);
    assert_eq!(
        weekly.map(|w| (w.used, w.resets_at)),
        Some((41, Some(ResetTime::from_epoch_secs(1_794_800_000))))
    );
    let scoped = |name: &str| {
        usage
            .windows
            .iter()
            .find(|w| w.scope.as_deref() == Some(name))
            .map(|w| (w.used, w.resets_at))
    };
    assert_eq!(
        scoped("Sonnet"),
        Some((12, Some(ResetTime::from_epoch_secs(1_794_800_000))))
    );
    assert_eq!(scoped("Fable").map(|(used, _)| used), Some(70));
    assert_eq!(scoped("Opus"), None);
    assert_eq!(scoped("Haiku"), None);
    assert_eq!(usage.windows.len(), 4);
}

#[test]
fn a_null_window_does_not_drop_the_others() {
    let usage = claude_plan_usage(&json!({"rate_limits": {
        "five_hour": {"utilization": null, "resets_at": null},
        "seven_day": {"utilization": 5.0, "resets_at": "2026-11-15T05:00:00Z"}
    }}))
    .unwrap_or_else(|failure| unreachable!("{failure:?}"));
    assert_eq!(usage.window(&WindowSpan::FiveHours), None);
    assert_eq!(usage.window(&WindowSpan::Weekly).map(|w| w.used), Some(5));
    assert_eq!(
        usage.primary().map(|w| w.span.clone()),
        Some(WindowSpan::Weekly)
    );
}

#[test]
fn no_subscription_and_unknown_shapes_are_told_apart() {
    assert_eq!(
        claude_plan_usage(&json!({"rate_limits_available": false, "rate_limits": null})),
        Err(UsageFailure::NoSubscription)
    );
    for odd in [
        json!({"rate_limits": null}),
        json!({"rate_limits": "soon"}),
        json!([1, 2]),
        json!({"rate_limits": {"five_hour": {"utilization": null}}}),
    ] {
        assert_eq!(
            claude_plan_usage(&odd),
            Err(UsageFailure::Unrecognised),
            "{odd}"
        );
    }
}

#[test]
fn the_limit_rows_are_read_only_when_no_named_window_is() {
    let usage = claude_plan_usage(&json!({"rate_limits": {"limits": [
        {"kind": "session", "percent": 30, "resets_at": "2026-11-09T18:30:00Z"},
        {"kind": "weekly_scoped", "percent": 70, "resets_at": null,
         "scope": {"model": {"display_name": "Opus"}, "surface": null}},
        {"kind": "session", "percent": "not a number"}
    ]}}))
    .unwrap_or_else(|failure| unreachable!("{failure:?}"));
    assert_eq!(
        usage.window(&WindowSpan::FiveHours).map(|w| w.used),
        Some(30)
    );
    assert_eq!(
        usage
            .windows
            .iter()
            .find(|w| w.scope.is_some())
            .map(|w| w.used),
        Some(70)
    );
    let named = claude_plan_usage(&reply()).unwrap_or_else(|failure| unreachable!("{failure:?}"));
    assert_eq!(
        named.window(&WindowSpan::FiveHours).map(|w| w.used),
        Some(24)
    );
}

#[test]
fn enabled_extra_usage_reads_as_a_monthly_window() {
    let usage = claude_plan_usage(&json!({"rate_limits": {
        "extra_usage": {"is_enabled": true, "utilization": 15.2}
    }}))
    .unwrap_or_else(|failure| unreachable!("{failure:?}"));
    assert_eq!(
        usage.window(&WindowSpan::Monthly),
        None,
        "extra usage is scoped"
    );
    assert_eq!(
        usage.windows.first().map(|w| (w.span.clone(), w.used)),
        Some((WindowSpan::Monthly, 15))
    );
}

#[test]
fn an_error_reply_reads_as_signed_out_rate_limited_or_unreachable() {
    assert_eq!(
        usage_failure_of("Not logged in · Please run /login"),
        UsageFailure::SignedOut
    );
    assert_eq!(
        usage_failure_of("429 Rate limit exceeded"),
        UsageFailure::RateLimited { retry_at: None }
    );
    assert_eq!(
        usage_failure_of("socket hang up"),
        UsageFailure::Unreachable
    );
    for signed_out in [
        "Authentication required",
        "401 Unauthorized",
        "auth: token expired",
    ] {
        assert_eq!(
            usage_failure_of(signed_out),
            UsageFailure::SignedOut,
            "{signed_out}"
        );
    }
    assert_eq!(
        usage_failure_of("the oauth apps endpoint timed out"),
        UsageFailure::Unreachable
    );
}

#[test]
fn a_reset_time_with_decimals_keeps_its_window() {
    let usage = claude_plan_usage(&json!({
        "rate_limits_available": true,
        "rate_limits": {
            "five_hour": {"utilization": 10, "resets_at": 1_794_249_000.75},
            "seven_day": {"utilization": 20, "resets_at": {"unexpected": true}}
        }
    }))
    .unwrap_or_else(|failure| unreachable!("{failure:?}"));
    assert_eq!(
        usage
            .window(&WindowSpan::FiveHours)
            .map(|w| (w.used, w.resets_at)),
        Some((10, Some(ResetTime::from_epoch_secs(1_794_249_000))))
    );
    assert_eq!(
        usage
            .window(&WindowSpan::Weekly)
            .map(|w| (w.used, w.resets_at)),
        Some((20, None))
    );
}
