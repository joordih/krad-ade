use krad_domain::{LimitAlert, LimitStatus, ResetTime};
use serde::Deserialize;
use serde_json::Value;
use std::time::SystemTime;

const RATE_LIMIT_EVENT: &str = "rate_limit_event";

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum RateLimitStatus {
    Allowed,
    AllowedWarning,
    Rejected,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RateLimitInfo {
    status: RateLimitStatus,
    #[serde(default)]
    resets_at: Option<u64>,
}

#[derive(Deserialize)]
struct RateLimitEvent {
    #[serde(rename = "type")]
    kind: String,
    rate_limit_info: RateLimitInfo,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LimitNews {
    Cleared,
    Raised(LimitAlert),
}

impl From<LimitNews> for Option<LimitAlert> {
    fn from(news: LimitNews) -> Self {
        match news {
            LimitNews::Cleared => None,
            LimitNews::Raised(alert) => Some(alert),
        }
    }
}

pub(crate) fn claude_limit_alert(value: &Value, heard_at: SystemTime) -> Option<LimitNews> {
    let event = RateLimitEvent::deserialize(value).ok()?;
    if event.kind != RATE_LIMIT_EVENT {
        return None;
    }
    let info = event.rate_limit_info;
    let status = match info.status {
        RateLimitStatus::Allowed => return Some(LimitNews::Cleared),
        RateLimitStatus::AllowedWarning => LimitStatus::Warning,
        RateLimitStatus::Rejected => LimitStatus::Reached,
    };
    Some(LimitNews::Raised(LimitAlert {
        status,
        resets_at: info.resets_at.map(ResetTime::from_epoch_secs),
        heard_at,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_rate_limit_event_warns_blocks_or_clears() {
        let event = |status: &str| {
            claude_limit_alert(
                &json!({"type": "rate_limit_event", "uuid": "u", "session_id": "s",
                "rate_limit_info": {"status": status, "resetsAt": 1_794_249_000, "utilization": 0.9}}),
                SystemTime::UNIX_EPOCH,
            )
        };
        assert_eq!(
            event("rejected"),
            Some(LimitNews::Raised(LimitAlert {
                status: LimitStatus::Reached,
                resets_at: Some(ResetTime::from_epoch_secs(1_794_249_000)),
                heard_at: SystemTime::UNIX_EPOCH,
            }))
        );
        assert_eq!(
            event("allowed_warning")
                .and_then(Option::<LimitAlert>::from)
                .map(|alert| alert.status),
            Some(LimitStatus::Warning)
        );
        assert_eq!(event("allowed"), Some(LimitNews::Cleared));
        assert_eq!(event("throttled"), None);
        assert_eq!(
            claude_limit_alert(&json!({"type": "assistant"}), SystemTime::UNIX_EPOCH),
            None
        );
    }
}
