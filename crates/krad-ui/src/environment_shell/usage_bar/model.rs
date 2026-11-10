use krad_domain::{
    AgentPreset, LimitAlert, LimitStatus, PlanUsage, ResetTime, UsageFailure, UsageReading,
    UsageWindow, WarnPercent, WindowSpan,
};
use std::borrow::Cow;
use std::time::{Duration, SystemTime};

const FULL_PERCENT: u16 = 100;
const PERCENT_SIGN: &str = "%";
const DAY_SUFFIX: &str = "d";
const HOUR_SUFFIX: &str = "h";
const MINUTE_SUFFIX: &str = "m";
const UNDER_A_MINUTE: &str = "<1m";
const LINE_BREAK: &str = "\n";
const DETAIL_SEPARATOR: &str = " · ";
const PLAN_LABEL: &str = "Plan";
const RESETS_TEXT: &str = "resets in";
const RESETS_NOW_TEXT: &str = "resets now";
const LIMITED_TEXT: &str = "limited";
const LIMIT_REACHED_TEXT: &str = "Limit reached";
const LIMIT_NEAR_TEXT: &str = "Close to the limit";
const FIVE_HOUR_LABEL: &str = "5 hours";
const WEEKLY_LABEL: &str = "Weekly";
const MONTHLY_LABEL: &str = "Monthly";
const ROLLING_LABEL: &str = "Rolling";
const READING_STATUS: &str = "Reading…";
const READING_HINT: &str = "Reading the subscription usage.";
const SIGNED_OUT_STATUS: &str = "Signed out";
const SIGNED_OUT_HINT: &str = "Sign in to the harness to see its subscription usage.";
const NO_PLAN_STATUS: &str = "No plan";
const NO_PLAN_HINT: &str =
    "This account has no subscription with usage limits, such as an API key.";
const RATE_LIMITED_STATUS: &str = "Rate limited";
const RATE_LIMITED_HINT: &str = "The harness refused to report usage for now; the app asks again";
const RATE_LIMITED_LATER: &str = "later";
const RATE_LIMITED_IN: &str = "in";
const UNREACHABLE_STATUS: &str = "Not available";
const UNREACHABLE_HINT: &str = "The harness did not answer; the app tries again in a while.";
const UNRECOGNISED_HINT: &str =
    "The harness answered in a shape this version of the app cannot read.";
const NO_SOURCE_HINT: &str = "Usage cannot be read on this host yet.";
const STARTING_STATUS: &str = "Starting…";
const STARTING_HINT: &str = "Waiting for the harness's server to start before reading its usage.";
const EMPTY_METER: Meter = Meter {
    used: 0,
    tone: Tone::Calm,
};
const STALE_HINT: &str = "Showing the last reading.";
const SECS_PER_MINUTE: u64 = 60;
const MINUTES_PER_HOUR: u64 = 60;
const HOURS_PER_DAY: u64 = 24;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tone {
    Calm,
    Warn,
    Limit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) enum UsageNote {
    Starting,
    HostNotReady,
    Broken(String),
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Meter {
    pub(super) used: u16,
    pub(super) tone: Tone,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Cell {
    pub(super) text: String,
    pub(super) tone: Tone,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct UsageBlock {
    pub(super) harness: AgentPreset,
    pub(super) meter: Meter,
    pub(super) cells: Vec<Cell>,
    pub(super) status: Option<&'static str>,
    pub(super) stale: bool,
    pub(super) tooltip: String,
}

#[derive(Clone, Copy)]
pub(super) struct BlockInput<'a> {
    pub(super) harness: AgentPreset,
    pub(super) reading: Option<&'a UsageReading>,
    pub(super) note: Option<&'a UsageNote>,
}

pub(super) fn left_text(left: Duration) -> String {
    let minutes = left.as_secs() / SECS_PER_MINUTE;
    let (hours, minutes) = (minutes / MINUTES_PER_HOUR, minutes % MINUTES_PER_HOUR);
    let (days, hours) = (hours / HOURS_PER_DAY, hours % HOURS_PER_DAY);
    match (days, hours, minutes) {
        (0, 0, 0) => UNDER_A_MINUTE.to_owned(),
        (0, 0, _) => format!("{minutes}{MINUTE_SUFFIX}"),
        (0, _, _) => format!("{hours}{HOUR_SUFFIX} {minutes}{MINUTE_SUFFIX}"),
        _ => format!("{days}{DAY_SUFFIX} {hours}{HOUR_SUFFIX}"),
    }
}

fn tone_of(window: &UsageWindow, warn_at: WarnPercent) -> Tone {
    if window.limited || window.used >= FULL_PERCENT {
        Tone::Limit
    } else if warn_at.reached_by(window.used) {
        Tone::Warn
    } else {
        Tone::Calm
    }
}

pub(super) fn window_cell(window: &UsageWindow, warn_at: WarnPercent, now: SystemTime) -> Cell {
    let mut text = format!("{}{PERCENT_SIGN}", window.used);
    let left = window.resets_at.map(|reset| reset.left(now));
    if let Some(left) = left.filter(|left| !left.is_zero()) {
        text.push(' ');
        text.push_str(&left_text(left));
    }
    Cell {
        text,
        tone: tone_of(window, warn_at),
    }
}

fn span_label(span: &WindowSpan) -> Cow<'_, str> {
    match span {
        WindowSpan::FiveHours => Cow::Borrowed(FIVE_HOUR_LABEL),
        WindowSpan::Weekly => Cow::Borrowed(WEEKLY_LABEL),
        WindowSpan::Monthly => Cow::Borrowed(MONTHLY_LABEL),
        WindowSpan::Rolling => Cow::Borrowed(ROLLING_LABEL),
        WindowSpan::Lasting(duration) => Cow::Owned(left_text(*duration)),
        WindowSpan::Named(name) => Cow::Borrowed(name),
    }
}

fn reset_text(reset: ResetTime, now: SystemTime) -> String {
    let left = reset.left(now);
    if left.is_zero() {
        return RESETS_NOW_TEXT.to_owned();
    }
    format!("{RESETS_TEXT} {}", left_text(left))
}

fn window_line(window: &UsageWindow, now: SystemTime) -> String {
    let span = span_label(&window.span);
    let mut line = match &window.scope {
        Some(scope) => format!("{span} ({scope}) {}{PERCENT_SIGN}", window.used),
        None => format!("{span} {}{PERCENT_SIGN}", window.used),
    };
    if window.limited {
        line.push_str(DETAIL_SEPARATOR);
        line.push_str(LIMITED_TEXT);
    }
    if let Some(reset) = window.resets_at {
        line.push_str(DETAIL_SEPARATOR);
        line.push_str(&reset_text(reset, now));
    }
    line
}

fn alert_line(alert: LimitAlert, now: SystemTime) -> String {
    let text = match alert.status {
        LimitStatus::Reached => LIMIT_REACHED_TEXT,
        LimitStatus::Warning => LIMIT_NEAR_TEXT,
    };
    match alert.resets_at {
        Some(reset) => format!("{text}{DETAIL_SEPARATOR}{}", reset_text(reset, now)),
        None => text.to_owned(),
    }
}

fn failure_status(failure: UsageFailure, now: SystemTime) -> (&'static str, String) {
    match failure {
        UsageFailure::SignedOut => (SIGNED_OUT_STATUS, SIGNED_OUT_HINT.to_owned()),
        UsageFailure::NoSubscription => (NO_PLAN_STATUS, NO_PLAN_HINT.to_owned()),
        UsageFailure::RateLimited { retry_at } => {
            let when = retry_at.map_or_else(
                || RATE_LIMITED_LATER.to_owned(),
                |at| format!("{RATE_LIMITED_IN} {}", left_text(at.left(now))),
            );
            (RATE_LIMITED_STATUS, format!("{RATE_LIMITED_HINT} {when}."))
        }
        UsageFailure::Unreachable => (UNREACHABLE_STATUS, UNREACHABLE_HINT.to_owned()),
        UsageFailure::Unrecognised => (UNREACHABLE_STATUS, UNRECOGNISED_HINT.to_owned()),
    }
}

fn usage_lines(harness: AgentPreset, usage: &PlanUsage, now: SystemTime) -> Vec<String> {
    let title = match &usage.plan {
        Some(plan) => format!(
            "{}{DETAIL_SEPARATOR}{PLAN_LABEL}: {plan}",
            harness.display_name()
        ),
        None => harness.display_name().to_owned(),
    };
    std::iter::once(title)
        .chain(usage.live_alert(now).map(|alert| alert_line(alert, now)))
        .chain(
            usage
                .ranked()
                .into_iter()
                .map(|window| window_line(window, now)),
        )
        .collect()
}

fn shown_block(
    harness: AgentPreset,
    usage: &PlanUsage,
    failure: Option<UsageFailure>,
    warn_at: WarnPercent,
    now: SystemTime,
) -> UsageBlock {
    let reached = usage
        .live_alert(now)
        .is_some_and(|alert| alert.status == LimitStatus::Reached);
    let meter = usage.primary().map_or(EMPTY_METER, |window| Meter {
        used: window.used.min(FULL_PERCENT),
        tone: if reached {
            Tone::Limit
        } else {
            tone_of(window, warn_at)
        },
    });
    let cells = usage
        .ranked()
        .into_iter()
        .filter(|window| window.scope.is_none())
        .map(|window| window_cell(window, warn_at, now))
        .collect();
    let mut lines = usage_lines(harness, usage, now);
    if let Some(failure) = failure {
        lines.push(failure_status(failure, now).1);
        lines.push(STALE_HINT.to_owned());
    }
    UsageBlock {
        harness,
        meter,
        cells,
        status: None,
        stale: failure.is_some(),
        tooltip: lines.join(LINE_BREAK),
    }
}

fn status_block(harness: AgentPreset, status: &'static str, hint: &str) -> UsageBlock {
    UsageBlock {
        harness,
        meter: EMPTY_METER,
        cells: Vec::new(),
        status: Some(status),
        stale: true,
        tooltip: [harness.display_name(), hint].join(LINE_BREAK),
    }
}

fn pending_block(harness: AgentPreset, note: Option<&UsageNote>) -> UsageBlock {
    match note {
        None => status_block(harness, READING_STATUS, READING_HINT),
        Some(UsageNote::Starting) => status_block(harness, STARTING_STATUS, STARTING_HINT),
        Some(UsageNote::HostNotReady) => status_block(harness, UNREACHABLE_STATUS, NO_SOURCE_HINT),
        Some(UsageNote::Broken(detail)) => status_block(harness, UNREACHABLE_STATUS, detail),
    }
}

pub(super) fn usage_block(
    input: BlockInput<'_>,
    warn_at: WarnPercent,
    now: SystemTime,
) -> UsageBlock {
    let BlockInput {
        harness,
        reading,
        note,
    } = input;
    let detail = match note {
        Some(UsageNote::Broken(detail)) => Some(detail.as_str()),
        Some(UsageNote::Starting | UsageNote::HostNotReady) | None => None,
    };
    let mut block = match reading.map(|reading| (reading.usage(), reading.failure())) {
        Some((Some(usage), failure)) => shown_block(harness, usage, failure, warn_at, now),
        Some((None, Some(failure))) if detail.is_none() => {
            let (status, hint) = failure_status(failure, now);
            status_block(harness, status, &hint)
        }
        Some((None, _)) | None => return pending_block(harness, note),
    };
    if let Some(detail) = detail.filter(|_| block.stale) {
        block.tooltip.push_str(LINE_BREAK);
        block.tooltip.push_str(detail);
    }
    block
}

#[cfg(test)]
mod tests;
