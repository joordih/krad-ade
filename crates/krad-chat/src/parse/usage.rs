use krad_domain::{PlanUsage, ResetTime, UsageFailure, UsageWindow, WindowSpan};
use serde::Deserialize;
use serde_json::Value;

const FULL_PERCENT: f64 = 100.0;
const OPUS_SCOPE: &str = "Opus";
const SONNET_SCOPE: &str = "Sonnet";
const OAUTH_APPS_SCOPE: &str = "OAuth apps";
const EXTRA_USAGE_SCOPE: &str = "Extra usage";
const SESSION_KIND: &str = "session";
const WEEKLY_KIND: &str = "weekly_all";
const WEEKLY_SCOPED_KIND: &str = "weekly_scoped";
const SIGNED_OUT_PHRASES: [&str; 1] = ["log in"];
const SIGNED_OUT_WORD_STARTS: [&str; 4] = ["login", "logged", "auth", "unauthori"];
const LARGEST_EPOCH: f64 = 1.0e18;
const RATE_LIMITED_HINTS: [&str; 2] = ["rate limit", "429"];

#[derive(Deserialize)]
#[serde(untagged)]
enum ResetField {
    Iso(String),
    Epoch(u64),
    FractionalEpoch(f64),
    Unknown(serde::de::IgnoredAny),
}

impl ResetField {
    fn time(&self) -> Option<ResetTime> {
        match self {
            Self::Iso(text) => ResetTime::parse_iso(text),
            Self::Epoch(value) => Some(ResetTime::from_epoch(*value)),
            Self::FractionalEpoch(value) => whole_epoch(*value).map(ResetTime::from_epoch),
            Self::Unknown(_) => None,
        }
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn whole_epoch(value: f64) -> Option<u64> {
    (value.is_finite() && (0.0..LARGEST_EPOCH).contains(&value)).then(|| value.trunc() as u64)
}

#[derive(Deserialize)]
struct Utilization {
    #[serde(default)]
    utilization: Option<f64>,
    #[serde(default)]
    resets_at: Option<ResetField>,
}

#[derive(Deserialize)]
struct ModelScoped {
    display_name: String,
    #[serde(default)]
    utilization: Option<f64>,
    #[serde(default)]
    resets_at: Option<ResetField>,
}

#[derive(Deserialize)]
struct ExtraUsage {
    #[serde(default)]
    is_enabled: bool,
    #[serde(default)]
    utilization: Option<f64>,
}

#[derive(Deserialize)]
struct Labelled {
    display_name: String,
}

#[derive(Default, Deserialize)]
struct Scope {
    #[serde(default)]
    model: Option<Labelled>,
    #[serde(default)]
    surface: Option<Labelled>,
}

#[derive(Deserialize)]
struct LimitRow {
    kind: String,
    percent: f64,
    #[serde(default)]
    resets_at: Option<ResetField>,
    #[serde(default)]
    scope: Option<Scope>,
}

#[derive(Default, Deserialize)]
struct RateLimits {
    #[serde(default)]
    five_hour: Option<Value>,
    #[serde(default)]
    seven_day: Option<Value>,
    #[serde(default)]
    seven_day_oauth_apps: Option<Value>,
    #[serde(default)]
    seven_day_opus: Option<Value>,
    #[serde(default)]
    seven_day_sonnet: Option<Value>,
    #[serde(default)]
    model_scoped: Option<Vec<Value>>,
    #[serde(default)]
    extra_usage: Option<Value>,
    #[serde(default)]
    limits: Option<Vec<Value>>,
}

#[derive(Deserialize)]
struct UsageReply {
    #[serde(default)]
    rate_limits: Option<Value>,
    #[serde(default)]
    rate_limits_available: Option<bool>,
    #[serde(default)]
    subscription_type: Option<String>,
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn percent(share: f64) -> Option<u16> {
    share
        .is_finite()
        .then(|| share.round().clamp(0.0, FULL_PERCENT) as u16)
}

fn window(
    span: WindowSpan,
    scope: Option<&str>,
    used: Option<f64>,
    resets_at: Option<&ResetField>,
) -> Option<UsageWindow> {
    Some(UsageWindow {
        span,
        scope: scope.map(str::to_owned),
        used: percent(used?)?,
        resets_at: resets_at.and_then(ResetField::time),
        limited: false,
    })
}

fn named_window(
    span: WindowSpan,
    scope: Option<&str>,
    slot: Option<&Value>,
) -> Option<UsageWindow> {
    let used = Utilization::deserialize(slot?).ok()?;
    window(span, scope, used.utilization, used.resets_at.as_ref())
}

fn scoped_window(entry: &Value) -> Option<UsageWindow> {
    let scoped = ModelScoped::deserialize(entry).ok()?;
    window(
        WindowSpan::Weekly,
        Some(&scoped.display_name),
        scoped.utilization,
        scoped.resets_at.as_ref(),
    )
}

fn extra_window(slot: Option<&Value>) -> Option<UsageWindow> {
    let extra = ExtraUsage::deserialize(slot?).ok()?;
    if !extra.is_enabled {
        return None;
    }
    window(
        WindowSpan::Monthly,
        Some(EXTRA_USAGE_SCOPE),
        extra.utilization,
        None,
    )
}

fn span_of(kind: &str) -> WindowSpan {
    match kind {
        SESSION_KIND => WindowSpan::FiveHours,
        WEEKLY_KIND | WEEKLY_SCOPED_KIND => WindowSpan::Weekly,
        other => WindowSpan::Named(other.to_owned()),
    }
}

fn row_window(row: &Value) -> Option<UsageWindow> {
    let row = LimitRow::deserialize(row).ok()?;
    let scope = row.scope.unwrap_or_default();
    let label = scope
        .model
        .or(scope.surface)
        .map(|label| label.display_name);
    window(
        span_of(&row.kind),
        label.as_deref(),
        Some(row.percent),
        row.resets_at.as_ref(),
    )
}

fn windows_of(limits: &RateLimits) -> Vec<UsageWindow> {
    let named = [
        named_window(WindowSpan::FiveHours, None, limits.five_hour.as_ref()),
        named_window(WindowSpan::Weekly, None, limits.seven_day.as_ref()),
        named_window(
            WindowSpan::Weekly,
            Some(OPUS_SCOPE),
            limits.seven_day_opus.as_ref(),
        ),
        named_window(
            WindowSpan::Weekly,
            Some(SONNET_SCOPE),
            limits.seven_day_sonnet.as_ref(),
        ),
        named_window(
            WindowSpan::Weekly,
            Some(OAUTH_APPS_SCOPE),
            limits.seven_day_oauth_apps.as_ref(),
        ),
    ];
    let scoped = limits.model_scoped.iter().flatten().map(scoped_window);
    let windows: Vec<UsageWindow> = named
        .into_iter()
        .chain(scoped)
        .chain([extra_window(limits.extra_usage.as_ref())])
        .flatten()
        .collect();
    if !windows.is_empty() {
        return windows;
    }
    limits
        .limits
        .iter()
        .flatten()
        .filter_map(row_window)
        .collect()
}

pub(crate) fn claude_plan_usage(reply: &Value) -> Result<PlanUsage, UsageFailure> {
    let reply = UsageReply::deserialize(reply).map_err(|_| UsageFailure::Unrecognised)?;
    if reply.rate_limits_available == Some(false) {
        return Err(UsageFailure::NoSubscription);
    }
    let limits = reply
        .rate_limits
        .as_ref()
        .and_then(|limits| RateLimits::deserialize(limits).ok())
        .unwrap_or_default();
    let mut usage = PlanUsage {
        plan: reply.subscription_type,
        ..PlanUsage::default()
    };
    for window in windows_of(&limits) {
        usage.place(window);
    }
    if usage.windows.is_empty() {
        return Err(UsageFailure::Unrecognised);
    }
    Ok(usage)
}

pub(crate) fn usage_failure_of(error: &str) -> UsageFailure {
    let error = error.to_lowercase();
    let mentions = |hints: &[&str]| hints.iter().any(|hint| error.contains(hint));
    let signed_out_word = error
        .split(|char: char| !char.is_alphanumeric())
        .any(|word| {
            SIGNED_OUT_WORD_STARTS
                .iter()
                .any(|start| word.starts_with(start))
        });
    if mentions(&RATE_LIMITED_HINTS) {
        UsageFailure::RateLimited { retry_at: None }
    } else if signed_out_word || mentions(&SIGNED_OUT_PHRASES) {
        UsageFailure::SignedOut
    } else {
        UsageFailure::Unreachable
    }
}

#[cfg(test)]
mod tests;
