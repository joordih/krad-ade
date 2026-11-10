use krad_domain::{PlanUsage, ResetTime, UsageFailure, UsageWindow, WindowSpan};
use serde::Deserialize;
use serde_json::Value;

use super::server::{OpencodeError, OpencodeServer};

const USAGE_LIMITS_PATH: &str = "/api/experimental/usage/limits";
const ENVELOPE_KEY: &str = "data";
const GO_PLAN: &str = "Go";
const FULL_PERCENT: f64 = 100.0;
const UNAUTHORIZED: u16 = 401;
const FORBIDDEN: u16 = 403;
const NOT_FOUND: u16 = 404;
const TOO_MANY_REQUESTS: u16 = 429;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meter {
    percent: f64,
    #[serde(default)]
    limited: bool,
    #[serde(default)]
    resets_at: Option<u64>,
}

#[derive(Deserialize)]
struct Go {
    #[serde(default)]
    rolling: Option<Value>,
    #[serde(default)]
    weekly: Option<Value>,
    #[serde(default)]
    monthly: Option<Value>,
}

#[derive(Deserialize)]
struct Limits {
    #[serde(default)]
    go: Option<Go>,
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn meter_window(span: WindowSpan, meter: Option<&Value>) -> Option<UsageWindow> {
    let meter = Meter::deserialize(meter?).ok()?;
    if !meter.percent.is_finite() {
        return None;
    }
    Some(UsageWindow {
        span,
        scope: None,
        used: meter.percent.round().clamp(0.0, FULL_PERCENT) as u16,
        resets_at: meter.resets_at.map(ResetTime::from_epoch),
        limited: meter.limited,
    })
}

pub(crate) fn opencode_plan_usage(reply: &Value) -> Result<PlanUsage, UsageFailure> {
    let body = reply.get(ENVELOPE_KEY).unwrap_or(reply);
    let limits = Limits::deserialize(body).map_err(|_| UsageFailure::Unrecognised)?;
    let go = limits.go.ok_or(UsageFailure::NoSubscription)?;
    let windows: Vec<UsageWindow> = [
        meter_window(WindowSpan::Rolling, go.rolling.as_ref()),
        meter_window(WindowSpan::Weekly, go.weekly.as_ref()),
        meter_window(WindowSpan::Monthly, go.monthly.as_ref()),
    ]
    .into_iter()
    .flatten()
    .collect();
    if windows.is_empty() {
        return Err(UsageFailure::Unrecognised);
    }
    Ok(PlanUsage {
        plan: Some(GO_PLAN.to_owned()),
        windows,
        alert: None,
    })
}

fn opencode_failure(error: &OpencodeError) -> UsageFailure {
    match error {
        OpencodeError::Http(ureq::Error::StatusCode(UNAUTHORIZED | FORBIDDEN)) => {
            UsageFailure::SignedOut
        }
        OpencodeError::Http(ureq::Error::StatusCode(TOO_MANY_REQUESTS)) => {
            UsageFailure::RateLimited { retry_at: None }
        }
        OpencodeError::Http(ureq::Error::StatusCode(NOT_FOUND)) => UsageFailure::Unrecognised,
        OpencodeError::Http(_)
        | OpencodeError::Io(_)
        | OpencodeError::NotStarted(_)
        | OpencodeError::Host(_) => UsageFailure::Unreachable,
    }
}

pub(crate) fn read_plan_usage(
    server: &OpencodeServer,
    directory: &str,
) -> Result<PlanUsage, UsageFailure> {
    let reply: Value = server
        .read_located(USAGE_LIMITS_PATH, directory)
        .map_err(|error| opencode_failure(&error))?;
    opencode_plan_usage(&reply)
}

/// # Errors
///
/// Fails when opencode cannot be reached, is signed out, has no Go subscription or answers in a
/// shape the app cannot read.
#[tracing::instrument(skip_all)]
pub fn read_opencode_usage(server: &OpencodeServer) -> Result<PlanUsage, UsageFailure> {
    let reply: Value = server
        .read(USAGE_LIMITS_PATH)
        .map_err(|error| opencode_failure(&error))?;
    opencode_plan_usage(&reply)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_go_meters_become_rolling_weekly_and_monthly_windows() {
        let usage = opencode_plan_usage(&json!({"data": {
            "go": {
                "rolling": {"percent": 12.4, "limited": false, "resetsAt": 1_794_249_000_000_u64},
                "weekly": {"percent": 100, "limited": true, "resetsAt": 1_794_800_000_000_u64},
                "monthly": null
            },
            "budgets": []
        }}))
        .unwrap_or_else(|failure| unreachable!("{failure:?}"));
        assert_eq!(usage.plan.as_deref(), Some("Go"));
        assert_eq!(
            usage.window(&WindowSpan::Rolling),
            Some(&UsageWindow {
                span: WindowSpan::Rolling,
                scope: None,
                used: 12,
                resets_at: Some(ResetTime::from_epoch_secs(1_794_249_000)),
                limited: false,
            })
        );
        assert_eq!(
            usage
                .window(&WindowSpan::Weekly)
                .map(|w| (w.used, w.limited)),
            Some((100, true))
        );
        assert_eq!(usage.window(&WindowSpan::Monthly), None);
    }

    #[test]
    fn a_reply_without_go_is_no_subscription_and_another_shape_unrecognised() {
        assert_eq!(
            opencode_plan_usage(&json!({"data": {"go": null, "budgets": []}})),
            Err(UsageFailure::NoSubscription)
        );
        assert_eq!(
            opencode_plan_usage(&json!({"data": {"budgets": []}})),
            Err(UsageFailure::NoSubscription)
        );
        assert_eq!(
            opencode_plan_usage(&json!({"go": {"rolling": {"limited": true}}})),
            Err(UsageFailure::Unrecognised)
        );
        assert_eq!(
            opencode_plan_usage(&json!("nope")),
            Err(UsageFailure::Unrecognised)
        );
        let bare = opencode_plan_usage(
            &json!({"go": {"monthly": {"percent": 5, "resetsAt": 1_794_249_000}}}),
        )
        .unwrap_or_else(|failure| unreachable!("{failure:?}"));
        assert_eq!(
            bare.window(&WindowSpan::Monthly)
                .map(|w| (w.used, w.resets_at)),
            Some((5, Some(ResetTime::from_epoch_secs(1_794_249_000))))
        );
    }

    #[test]
    fn http_failures_read_as_signed_out_rate_limited_or_unreachable() {
        let status = |code| opencode_failure(&OpencodeError::Http(ureq::Error::StatusCode(code)));
        assert_eq!(status(401), UsageFailure::SignedOut);
        assert_eq!(status(429), UsageFailure::RateLimited { retry_at: None });
        assert_eq!(status(404), UsageFailure::Unrecognised);
        assert_eq!(status(502), UsageFailure::Unreachable);
        let gone = OpencodeError::NotStarted("exited".to_owned());
        assert_eq!(opencode_failure(&gone), UsageFailure::Unreachable);
    }
}
