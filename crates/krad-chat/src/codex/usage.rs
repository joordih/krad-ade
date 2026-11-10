use krad_domain::{PlanUsage, UsageFailure};
use std::path::Path;
use std::sync::mpsc::channel;

use super::limits::{LimitBook, LimitsRead, ReadParams, RATE_LIMITS_READ};
use super::rpc::{AppServer, CodexError};
use crate::parse::usage_failure_of;
use crate::process::Runner;

fn codex_failure(error: CodexError) -> UsageFailure {
    match error {
        CodexError::Io(_) | CodexError::Gone => UsageFailure::Unreachable,
        CodexError::Shape(_) => UsageFailure::Unrecognised,
        CodexError::Refused(message) => usage_failure_of(&message),
    }
}

fn usage_of(read: LimitsRead) -> Result<PlanUsage, UsageFailure> {
    let usage = LimitBook::default().read(read);
    if usage.windows.is_empty() {
        return Err(UsageFailure::NoSubscription);
    }
    Ok(usage)
}

/// # Errors
///
/// Fails when codex cannot be started, is signed out, has no subscription limits or answers in a
/// shape the app cannot read.
#[tracing::instrument(skip_all)]
pub fn read_codex_usage(runner: &Runner, cwd: &Path) -> Result<PlanUsage, UsageFailure> {
    let (pushed, _unheard) = channel();
    let server = AppServer::start(runner, cwd, &[], &[], pushed).map_err(codex_failure)?;
    let read = server
        .call::<_, LimitsRead>(RATE_LIMITS_READ, &ReadParams::default())
        .map_err(codex_failure)?;
    usage_of(read)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_reading_without_windows_is_no_subscription() {
        let read: LimitsRead =
            serde_json::from_value(json!({"rateLimits": null})).unwrap_or_default();
        assert_eq!(usage_of(read), Err(UsageFailure::NoSubscription));
        let read: LimitsRead = serde_json::from_value(json!({"rateLimits": {"limitId": "codex",
            "primary": {"usedPercent": 7}}}))
        .unwrap_or_default();
        assert_eq!(usage_of(read).map(|usage| usage.windows.len()), Ok(1));
    }

    #[test]
    fn refusals_read_as_signed_out_or_rate_limited() {
        let refused = |text: &str| codex_failure(CodexError::Refused(text.to_owned()));
        assert_eq!(
            refused("codex account authentication required"),
            UsageFailure::SignedOut
        );
        assert_eq!(
            refused("rate limit reached"),
            UsageFailure::RateLimited { retry_at: None }
        );
        assert_eq!(codex_failure(CodexError::Gone), UsageFailure::Unreachable);
    }
}
