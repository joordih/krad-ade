mod model;
mod view;

use gpui_kit::{AppContext as _, Context, Task};
use krad_agents::AgentStatus;
use krad_chat::{read_claude_usage, read_codex_usage, read_opencode_usage, OpencodeServer, Runner};
use krad_domain::{
    AgentPreset, Host, LimitAlert, PlanUsage, UsageBudget, UsageCadence, UsageFailure,
    UsageIndicator, UsageReading, UsageSchedule, UsageTrigger, UsageWindow, WindowSpan,
};
use krad_host::HostLink;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use super::chat::StartError;
use super::opencode::OpencodeLink;
use super::EnvironmentShell;
use model::UsageNote;

pub(super) use view::USAGE_BAR_H;

const USAGE_TICK: Duration = Duration::from_secs(30);
const USAGE_HARNESSES: [AgentPreset; 3] = [
    AgentPreset::ClaudeCode,
    AgentPreset::Codex,
    AgentPreset::Opencode,
];
const STATUSLINE_HARNESS: AgentPreset = AgentPreset::ClaudeCode;
const OPENCODE_FAILED_TEXT: &str = "The opencode server did not start";

type UsageKey = (Host, AgentPreset);

pub(super) enum UsageNews {
    Read(PlanUsage),
    Alert(Option<LimitAlert>),
    Statusline(PlanUsage),
}

#[derive(Default)]
pub(super) struct UsageBoard {
    readings: HashMap<UsageKey, UsageReading>,
    schedules: HashMap<UsageKey, UsageSchedule>,
    notes: HashMap<UsageKey, UsageNote>,
    claude_budget: Arc<UsageBudget>,
    ticker: Option<Task<()>>,
}

enum SourceState {
    Ready(UsageSource),
    Waiting(UsageNote),
    Broken(String),
}

enum UsageSource {
    Claude(Runner, PathBuf),
    Codex(Runner, PathBuf),
    Opencode(Arc<OpencodeServer>),
}

fn claims_budget(source: &UsageSource, budget: &UsageBudget, now: Instant) -> bool {
    match source {
        UsageSource::Claude(..) => budget.try_spend(now, UsageCadence::of(AgentPreset::ClaudeCode)),
        UsageSource::Codex(..) | UsageSource::Opencode(_) => true,
    }
}

impl UsageSource {
    fn read(self) -> Result<PlanUsage, UsageFailure> {
        match self {
            Self::Claude(runner, cwd) => read_claude_usage(&runner, &cwd),
            Self::Codex(runner, cwd) => read_codex_usage(&runner, &cwd),
            Self::Opencode(server) => read_opencode_usage(&server),
        }
    }
}

fn statusline_usage(status: &AgentStatus) -> Option<PlanUsage> {
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

impl EnvironmentShell {
    pub(super) fn usage_bar_shown(&self) -> bool {
        self.settings.plan_usage.indicator == UsageIndicator::Shown
    }

    pub(super) fn usage_bar_room(&self) -> f32 {
        if self.usage_bar_shown() {
            USAGE_BAR_H
        } else {
            0.0
        }
    }

    pub(super) fn toggle_usage_bar(&mut self, cx: &mut Context<Self>) {
        self.settings.plan_usage.indicator = if self.usage_bar_shown() {
            UsageIndicator::Hidden
        } else {
            UsageIndicator::Shown
        };
        self.save_settings(cx);
        self.read_due_usage(UsageTrigger::Timer, cx);
        cx.notify();
    }

    pub(super) fn watch_usage(&mut self, cx: &mut Context<Self>) {
        if self.usage.ticker.is_some() {
            return;
        }
        self.usage.ticker = Some(cx.spawn(async move |this, cx| loop {
            let ticked = this.update(cx, |shell, cx| {
                shell.read_due_usage(UsageTrigger::Timer, cx);
                cx.notify();
            });
            if ticked.is_err() {
                break;
            }
            cx.background_executor().timer(USAGE_TICK).await;
        }));
    }

    pub(super) fn claude_usage_budget(&self) -> Arc<UsageBudget> {
        Arc::clone(&self.usage.claude_budget)
    }

    pub(super) fn read_usage_on_time(&mut self, cx: &mut Context<Self>) {
        self.read_due_usage(UsageTrigger::Timer, cx);
    }

    pub(super) fn refresh_usage(&mut self, cx: &mut Context<Self>) {
        self.read_due_usage(UsageTrigger::Refresh, cx);
        cx.notify();
    }

    fn installed_on(&self, link: &HostLink, harness: AgentPreset) -> bool {
        if link.is_local() {
            return self.native_agents.contains(&harness);
        }
        self.host_agents
            .get(&link.host())
            .is_some_and(|found| found.contains(&harness))
    }

    fn usage_harnesses(&self, link: &HostLink) -> Vec<AgentPreset> {
        let host = link.host();
        USAGE_HARNESSES
            .into_iter()
            .filter(|harness| {
                self.installed_on(link, *harness)
                    || self.usage.readings.contains_key(&(host.clone(), *harness))
            })
            .collect()
    }

    fn opencode_usage_source(&mut self, link: &HostLink, cx: &mut Context<Self>) -> SourceState {
        let opened = if link.is_local() {
            Some(&self.opencode.link)
        } else {
            self.opencode.hosts.get(&link.host())
        };
        match opened {
            Some(OpencodeLink::Ready { server, .. }) => {
                SourceState::Ready(UsageSource::Opencode(Arc::clone(server)))
            }
            Some(OpencodeLink::Starting) => SourceState::Waiting(UsageNote::Starting),
            Some(OpencodeLink::Failed(error)) => {
                SourceState::Broken(format!("{OPENCODE_FAILED_TEXT}: {error}"))
            }
            Some(OpencodeLink::Absent) | None => {
                self.start_opencode_for_usage(link, cx);
                SourceState::Waiting(UsageNote::Starting)
            }
        }
    }

    fn start_opencode_for_usage(&mut self, link: &HostLink, cx: &mut Context<Self>) {
        if link.is_local() {
            return self.ensure_opencode(cx);
        }
        match self.harness_runner(link, AgentPreset::Opencode, None) {
            Ok((runner, cwd)) => self.ensure_host_opencode(runner, cwd, cx),
            Err(error) => {
                tracing::debug!(%error, "opencode cannot start on the host for usage yet");
            }
        }
    }

    fn runner_source(
        &self,
        link: &HostLink,
        harness: AgentPreset,
        source: fn(Runner, PathBuf) -> UsageSource,
    ) -> SourceState {
        match self.harness_runner(link, harness, None) {
            Ok((runner, cwd)) => SourceState::Ready(source(runner, cwd)),
            Err(StartError::Waiting) => SourceState::Waiting(UsageNote::HostNotReady),
            Err(error) => SourceState::Broken(error.to_string()),
        }
    }

    fn usage_source(
        &mut self,
        link: &HostLink,
        harness: AgentPreset,
        cx: &mut Context<Self>,
    ) -> SourceState {
        match harness {
            AgentPreset::Opencode => self.opencode_usage_source(link, cx),
            AgentPreset::ClaudeCode => self.runner_source(link, harness, UsageSource::Claude),
            AgentPreset::Codex => self.runner_source(link, harness, UsageSource::Codex),
            AgentPreset::Gateway | AgentPreset::Shell | AgentPreset::Acp(_) => {
                SourceState::Waiting(UsageNote::HostNotReady)
            }
        }
    }

    fn read_due_usage(&mut self, trigger: UsageTrigger, cx: &mut Context<Self>) {
        if !self.usage_bar_shown() {
            return;
        }
        let link = self.active_link();
        let now = Instant::now();
        for harness in self.usage_harnesses(&link) {
            let key = (link.host(), harness);
            let due = self
                .usage
                .schedules
                .get(&key)
                .is_none_or(|schedule| schedule.due(now, UsageCadence::of(harness), trigger));
            if !due {
                continue;
            }
            match self.usage_source(&link, harness, cx) {
                SourceState::Ready(source) => {
                    if !claims_budget(&source, &self.usage.claude_budget, now) {
                        tracing::trace!("plan usage read within the shared budget gap; skipped");
                        continue;
                    }
                    self.usage.notes.remove(&key);
                    self.usage
                        .schedules
                        .entry(key.clone())
                        .or_default()
                        .started(now);
                    Self::read_usage(key, source, cx);
                }
                SourceState::Waiting(note) => {
                    tracing::debug!(?harness, ?note, "plan usage waits for its harness");
                    self.usage.notes.insert(key, note);
                }
                SourceState::Broken(detail) => {
                    tracing::debug!(?harness, %detail, "plan usage has no way to be read");
                    self.usage
                        .notes
                        .insert(key.clone(), UsageNote::Broken(detail));
                    self.usage
                        .schedules
                        .entry(key.clone())
                        .or_default()
                        .started(now);
                    self.usage_read(key, Err(UsageFailure::Unreachable));
                }
            }
        }
    }

    fn read_usage(key: UsageKey, source: UsageSource, cx: &mut Context<Self>) {
        let read = cx.background_spawn(async move { source.read() });
        cx.spawn(async move |this, cx| {
            let read = read.await;
            this.update(cx, |shell, cx| {
                shell.usage_read(key, read);
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn usage_read(&mut self, key: UsageKey, read: Result<PlanUsage, UsageFailure>) {
        match &read {
            Ok(usage) => {
                tracing::debug!(harness = ?key.1, windows = usage.windows.len(), "plan usage read");
            }
            Err(failure) => tracing::debug!(harness = ?key.1, ?failure, "plan usage not read"),
        }
        let failure = read.as_ref().err().copied();
        self.usage
            .schedules
            .entry(key.clone())
            .or_default()
            .finished(Instant::now(), SystemTime::now(), failure);
        let previous = self.usage.readings.remove(&key);
        self.usage
            .readings
            .insert(key, UsageReading::after(previous, read, SystemTime::now()));
    }

    pub(super) fn usage_heard(&mut self, host: Host, harness: AgentPreset, news: UsageNews) {
        let key = (host, harness);
        let previous = self.usage.readings.remove(&key);
        let spent_budget = matches!(news, UsageNews::Read(_));
        let reading = match news {
            UsageNews::Read(usage) => {
                Some(UsageReading::after(previous, Ok(usage), SystemTime::now()))
            }
            UsageNews::Statusline(usage) => Some(UsageReading::merged(previous, usage)),
            UsageNews::Alert(alert) => UsageReading::alerted(previous, alert),
        };
        let Some(reading) = reading else {
            return;
        };
        if matches!(reading, UsageReading::Fresh(_)) {
            let schedule = self.usage.schedules.entry(key.clone()).or_default();
            if spent_budget {
                schedule.read_elsewhere(Instant::now());
            } else {
                schedule.heard(Instant::now());
            }
        }
        self.usage.readings.insert(key, reading);
    }

    pub(super) fn statusline_heard(&mut self, host: Host, status: &AgentStatus) {
        if let Some(usage) = statusline_usage(status) {
            self.usage_heard(host, STATUSLINE_HARNESS, UsageNews::Statusline(usage));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAST_THE_GAP: Duration = Duration::from_mins(10);

    #[test]
    fn a_claude_read_waits_for_the_budget_its_chats_share_and_codex_never_does() {
        let budget = UsageBudget::default();
        let now = Instant::now();
        let claude =
            || UsageSource::Claude(Runner::Native(PathBuf::from("claude")), PathBuf::new());
        let codex = UsageSource::Codex(Runner::Native(PathBuf::from("codex")), PathBuf::new());
        assert!(budget.try_spend(now, UsageCadence::of(AgentPreset::ClaudeCode)));
        assert!(!claims_budget(&claude(), &budget, now));
        assert!(claims_budget(&codex, &budget, now));
        let later = now + PAST_THE_GAP;
        assert!(claims_budget(&claude(), &budget, later));
    }
    use krad_domain::ResetTime;

    #[test]
    fn the_statusline_reports_the_five_hour_and_weekly_windows_it_carries() {
        let status = AgentStatus {
            five_hour_used: Some(30),
            five_hour_resets_at: Some(ResetTime::from_epoch_secs(100)),
            seven_day_used: None,
            ..AgentStatus::default()
        };
        let usage = statusline_usage(&status);
        assert_eq!(
            usage.map(|usage| usage.windows),
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
