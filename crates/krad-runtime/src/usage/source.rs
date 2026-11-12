use krad_chat::{read_claude_usage, read_codex_usage, read_opencode_usage, OpencodeServer, Runner};
use krad_domain::{
    AgentPreset, AgentStatus, Host, PlanUsage, UsageBudget, UsageCadence, UsageFailure,
    UsageWindow, WindowSpan,
};
use krad_protocol::UsageNote;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone)]
pub enum UsageSource {
    Claude(Runner, PathBuf),
    Codex(Runner, PathBuf),
    Opencode(Arc<OpencodeServer>),
}

impl fmt::Debug for UsageSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Claude(runner, cwd) => formatter
                .debug_tuple("Claude")
                .field(runner)
                .field(cwd)
                .finish(),
            Self::Codex(runner, cwd) => formatter
                .debug_tuple("Codex")
                .field(runner)
                .field(cwd)
                .finish(),
            Self::Opencode(_) => formatter.write_str("Opencode"),
        }
    }
}

impl UsageSource {
    pub(super) fn claims(&self, budget: &UsageBudget, now: Instant) -> bool {
        match self {
            Self::Claude(..) => budget.try_spend(now, UsageCadence::of(AgentPreset::ClaudeCode)),
            Self::Codex(..) | Self::Opencode(_) => true,
        }
    }

    pub(super) fn read(self) -> Result<PlanUsage, UsageFailure> {
        match self {
            Self::Claude(runner, cwd) => read_claude_usage(&runner, &cwd),
            Self::Codex(runner, cwd) => read_codex_usage(&runner, &cwd),
            Self::Opencode(server) => read_opencode_usage(&server),
        }
    }
}

#[derive(Clone, Debug)]
pub enum SourceState {
    Ready(UsageSource),
    Waiting(UsageNote),
    Broken(String),
}

#[derive(Clone, Debug)]
pub struct UsagePlace {
    pub host: Host,
    pub sources: Vec<(AgentPreset, SourceState)>,
}

pub(super) fn statusline_usage(status: &AgentStatus) -> Option<PlanUsage> {
    let reported = [
        (
            WindowSpan::FiveHours,
            status.five_hour_used,
            status.five_hour_resets_at,
        ),
        (
            WindowSpan::Weekly,
            status.seven_day_used,
            status.seven_day_resets_at,
        ),
    ];
    let windows: Vec<UsageWindow> = reported
        .into_iter()
        .filter_map(|(span, used, resets_at)| {
            Some(UsageWindow {
                span,
                scope: None,
                used: used?,
                resets_at,
                limited: false,
            })
        })
        .collect();
    (!windows.is_empty()).then(|| PlanUsage {
        windows,
        ..PlanUsage::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{ResetTime, UsageReading};
    use std::time::Duration;

    const PAST_THE_GAP: Duration = Duration::from_mins(10);

    #[test]
    fn a_claude_read_waits_for_the_budget_its_chats_share_and_codex_never_does() {
        let budget = UsageBudget::default();
        let now = Instant::now();
        let claude =
            || UsageSource::Claude(Runner::Native(PathBuf::from("claude")), PathBuf::new());
        let codex = UsageSource::Codex(Runner::Native(PathBuf::from("codex")), PathBuf::new());
        assert!(budget.try_spend(now, UsageCadence::of(AgentPreset::ClaudeCode)));
        assert!(!claude().claims(&budget, now));
        assert!(codex.claims(&budget, now));
        assert!(claude().claims(&budget, now + PAST_THE_GAP));
    }

    #[test]
    fn the_statusline_reports_the_five_hour_and_weekly_windows_it_carries() {
        let status = AgentStatus {
            five_hour_used: Some(30),
            five_hour_resets_at: Some(ResetTime::from_epoch_secs(100)),
            seven_day_used: None,
            ..AgentStatus::default()
        };
        assert_eq!(
            statusline_usage(&status).map(|usage| usage.windows),
            Some(vec![UsageWindow {
                span: WindowSpan::FiveHours,
                scope: None,
                used: 30,
                resets_at: Some(ResetTime::from_epoch_secs(100)),
                limited: false,
            }])
        );
        assert_eq!(statusline_usage(&AgentStatus::default()), None);
    }

    #[test]
    fn a_statusline_with_one_window_keeps_the_other_and_its_reset() {
        let first = AgentStatus {
            five_hour_used: Some(10),
            five_hour_resets_at: Some(ResetTime::from_epoch_secs(5)),
            seven_day_used: Some(40),
            ..AgentStatus::default()
        };
        let weekly_only = AgentStatus {
            seven_day_used: Some(45),
            ..AgentStatus::default()
        };
        let heard = |previous, status: &AgentStatus| {
            statusline_usage(status).map(|usage| UsageReading::merged(previous, usage))
        };
        let merged = heard(heard(None, &first), &weekly_only);
        let window = |span| {
            merged
                .as_ref()
                .and_then(UsageReading::usage)
                .and_then(|usage| usage.window(&span))
                .map(|window| (window.used, window.resets_at))
        };
        assert_eq!(
            window(WindowSpan::FiveHours),
            Some((10, Some(ResetTime::from_epoch_secs(5))))
        );
        assert_eq!(window(WindowSpan::Weekly), Some((45, None)));
    }
}
