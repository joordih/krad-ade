use krad_chat::{harness_health, harness_off_path, Runner};
use krad_domain::{AgentPreset, HarnessHealth};
use krad_host::HostLink;
use krad_protocol::{EventKind, HarnessCheck, HarnessChecked, HealthPlace, HealthSnapshot};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::jobs::{JobDone, Jobs};

#[derive(Clone, Debug)]
pub enum HarnessProbe {
    Run(Runner, PathBuf),
    OffPath {
        link: HostLink,
        home: Option<PathBuf>,
        program: String,
    },
    Missing,
}

impl HarnessProbe {
    fn run(self, harness: AgentPreset) -> HarnessHealth {
        match self {
            Self::Run(runner, cwd) => harness_health(&runner, &cwd, harness),
            Self::OffPath {
                link,
                home,
                program,
            } => harness_off_path(&link, home.as_deref(), harness, &program),
            Self::Missing => {
                tracing::debug!(
                    harness = harness.display_name(),
                    "harness not found on PATH"
                );
                HarnessHealth::Missing
            }
        }
    }
}

type HealthKey = (HealthPlace, AgentPreset);

#[derive(Debug, Default)]
pub(crate) struct Health {
    checked: HashMap<HealthKey, HarnessCheck>,
}

fn told(place: &HealthPlace, harness: AgentPreset, check: HarnessCheck) -> EventKind {
    EventKind::HarnessChecked(HarnessChecked {
        place: place.clone(),
        harness,
        check,
    })
}

impl Health {
    pub(crate) fn check(
        &mut self,
        place: &HealthPlace,
        probes: Vec<(AgentPreset, Option<HarnessProbe>)>,
        jobs: &Jobs,
        outbox: &mut Vec<EventKind>,
    ) {
        for (harness, probe) in probes {
            let key = (place.clone(), harness);
            if self.checked.get(&key) == Some(&HarnessCheck::Checking) {
                continue;
            }
            let Some(probe) = probe else {
                self.forget(&key, outbox);
                continue;
            };
            let job_place = place.clone();
            let started = jobs.run(move || JobDone::HarnessChecked {
                place: job_place,
                harness,
                health: probe.run(harness),
            });
            if let Err(error) = started {
                tracing::warn!(%error, "a harness check could not start");
                self.forget(&key, outbox);
                continue;
            }
            self.checked.insert(key, HarnessCheck::Checking);
            outbox.push(told(place, harness, HarnessCheck::Checking));
        }
    }

    fn forget(&mut self, key: &HealthKey, outbox: &mut Vec<EventKind>) {
        if self.checked.remove(key).is_some() {
            outbox.push(told(&key.0, key.1, HarnessCheck::Forgotten));
        }
    }

    pub(crate) fn checked(
        &mut self,
        place: HealthPlace,
        harness: AgentPreset,
        health: HarnessHealth,
        outbox: &mut Vec<EventKind>,
    ) {
        let check = HarnessCheck::Done(health);
        outbox.push(told(&place, harness, check.clone()));
        self.checked.insert((place, harness), check);
    }

    pub(crate) fn snapshot(&self) -> HealthSnapshot {
        HealthSnapshot {
            checks: self
                .checked
                .iter()
                .map(|((place, harness), check)| HarnessChecked {
                    place: place.clone(),
                    harness: *harness,
                    check: check.clone(),
                })
                .collect(),
        }
    }
}
