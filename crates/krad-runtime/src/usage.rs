mod source;

use krad_domain::{
    AgentPreset, Host, LimitAlert, PlanUsage, UsageBudget, UsageCadence, UsageFailure,
    UsageReading, UsageSchedule, UsageTrigger,
};
use krad_protocol::{
    EventKind, Reply, UsageCommand, UsageNews, UsageNote, UsageShown, UsageSnapshot,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use crate::jobs::{JobDone, Jobs};
use source::statusline_usage;

pub use source::{SourceState, UsagePlace, UsageSource};

const USAGE_TICK: Duration = Duration::from_secs(30);
const STATUSLINE_HARNESS: AgentPreset = AgentPreset::ClaudeCode;

pub(crate) type UsageKey = (Host, AgentPreset);

#[derive(Debug)]
pub(crate) struct Usage {
    readings: HashMap<UsageKey, UsageReading>,
    schedules: HashMap<UsageKey, UsageSchedule>,
    notes: HashMap<UsageKey, UsageNote>,
    budget: Arc<UsageBudget>,
    place: Option<UsagePlace>,
    next_timer: Instant,
}

enum Heard {
    Read(PlanUsage),
    Alert(Option<LimitAlert>),
    Statusline(PlanUsage),
}

impl Usage {
    pub(crate) fn new(budget: Arc<UsageBudget>) -> Self {
        Self {
            readings: HashMap::new(),
            schedules: HashMap::new(),
            notes: HashMap::new(),
            budget,
            place: None,
            next_timer: Instant::now() + USAGE_TICK,
        }
    }

    pub(crate) fn execute(
        &mut self,
        command: &UsageCommand,
        jobs: &Jobs,
        outbox: &mut Vec<EventKind>,
    ) -> Option<Reply> {
        match command {
            UsageCommand::Refresh => self.read_due(UsageTrigger::Refresh, jobs, outbox),
            UsageCommand::Heard {
                host,
                harness,
                news,
            } => {
                let heard = match news {
                    UsageNews::Read { usage } => Heard::Read(usage.clone()),
                    UsageNews::Alert { alert } => Heard::Alert(*alert),
                };
                self.heard(&(host.clone(), *harness), heard, outbox);
            }
            UsageCommand::Statusline { host, status } => {
                if let Some(usage) = statusline_usage(status) {
                    let key = (host.clone(), STATUSLINE_HARNESS);
                    self.heard(&key, Heard::Statusline(usage), outbox);
                }
            }
            UsageCommand::Unknown => return None,
        }
        Some(Reply::Accepted)
    }

    pub(crate) fn place(
        &mut self,
        place: Option<UsagePlace>,
        jobs: &Jobs,
        outbox: &mut Vec<EventKind>,
    ) {
        self.place = place;
        self.read_due(UsageTrigger::Timer, jobs, outbox);
    }

    pub(crate) fn tick(&mut self, jobs: &Jobs, outbox: &mut Vec<EventKind>) {
        let now = Instant::now();
        if now < self.next_timer {
            return;
        }
        self.next_timer = now + USAGE_TICK;
        self.read_due(UsageTrigger::Timer, jobs, outbox);
    }

    fn read_due(&mut self, trigger: UsageTrigger, jobs: &Jobs, outbox: &mut Vec<EventKind>) {
        let Some(place) = self.place.clone() else {
            return;
        };
        let now = Instant::now();
        for (harness, state) in place.sources {
            let key = (place.host.clone(), harness);
            let due = self
                .schedules
                .get(&key)
                .is_none_or(|schedule| schedule.due(now, UsageCadence::of(harness), trigger));
            if due {
                self.read_one(&key, state, jobs, outbox);
            }
        }
    }

    fn read_one(
        &mut self,
        key: &UsageKey,
        state: SourceState,
        jobs: &Jobs,
        outbox: &mut Vec<EventKind>,
    ) {
        let now = Instant::now();
        match state {
            SourceState::Ready(source) => {
                if !source.claims(&self.budget, now) {
                    tracing::trace!("plan usage read within the shared budget gap; skipped");
                    return;
                }
                self.note(key, None, outbox);
                self.schedules.entry(key.clone()).or_default().started(now);
                let job_key = key.clone();
                let started = jobs.run(move || JobDone::UsageRead {
                    key: job_key,
                    read: source.read(),
                });
                if let Err(error) = started {
                    tracing::warn!(%error, "a plan usage read could not start");
                    self.read(key, Err(UsageFailure::Unreachable), outbox);
                }
            }
            SourceState::Waiting(note) => {
                tracing::debug!(harness = ?key.1, ?note, "plan usage waits for its harness");
                self.note(key, Some(note), outbox);
            }
            SourceState::Broken(detail) => {
                tracing::debug!(harness = ?key.1, %detail, "plan usage has no way to be read");
                self.note(key, Some(UsageNote::Broken(detail)), outbox);
                self.schedules.entry(key.clone()).or_default().started(now);
                self.read(key, Err(UsageFailure::Unreachable), outbox);
            }
        }
    }

    fn note(&mut self, key: &UsageKey, note: Option<UsageNote>, outbox: &mut Vec<EventKind>) {
        let unchanged = match note {
            Some(note) => self.notes.insert(key.clone(), note.clone()) == Some(note),
            None => self.notes.remove(key).is_none(),
        };
        if !unchanged {
            outbox.push(self.shown(key));
        }
    }

    pub(crate) fn read(
        &mut self,
        key: &UsageKey,
        read: Result<PlanUsage, UsageFailure>,
        outbox: &mut Vec<EventKind>,
    ) {
        match &read {
            Ok(usage) => {
                tracing::debug!(harness = ?key.1, windows = usage.windows.len(), "plan usage read");
            }
            Err(failure) => tracing::debug!(harness = ?key.1, ?failure, "plan usage not read"),
        }
        let failure = read.as_ref().err().copied();
        self.schedules.entry(key.clone()).or_default().finished(
            Instant::now(),
            SystemTime::now(),
            failure,
        );
        let previous = self.readings.remove(key);
        self.readings.insert(
            key.clone(),
            UsageReading::after(previous, read, SystemTime::now()),
        );
        outbox.push(self.shown(key));
    }

    fn heard(&mut self, key: &UsageKey, heard: Heard, outbox: &mut Vec<EventKind>) {
        let previous = self.readings.remove(key);
        let spent_budget = matches!(heard, Heard::Read(_));
        let reading = match heard {
            Heard::Read(usage) => Some(UsageReading::after(previous, Ok(usage), SystemTime::now())),
            Heard::Statusline(usage) => Some(UsageReading::merged(previous, usage)),
            Heard::Alert(alert) => UsageReading::alerted(previous, alert),
        };
        let Some(reading) = reading else {
            return;
        };
        if matches!(reading, UsageReading::Fresh(_)) {
            let schedule = self.schedules.entry(key.clone()).or_default();
            if spent_budget {
                schedule.read_elsewhere(Instant::now());
            } else {
                schedule.heard(Instant::now());
            }
        }
        self.readings.insert(key.clone(), reading);
        outbox.push(self.shown(key));
    }

    fn shown(&self, key: &UsageKey) -> EventKind {
        EventKind::UsageShown(self.shown_of(key))
    }

    fn shown_of(&self, key: &UsageKey) -> UsageShown {
        UsageShown {
            host: key.0.clone(),
            harness: key.1,
            reading: self.readings.get(key).cloned(),
            note: self.notes.get(key).cloned(),
        }
    }

    pub(crate) fn snapshot(&self) -> UsageSnapshot {
        let keys: HashSet<&UsageKey> = self.readings.keys().chain(self.notes.keys()).collect();
        UsageSnapshot {
            shown: keys.into_iter().map(|key| self.shown_of(key)).collect(),
        }
    }
}
