use krad_domain::CloudRunnerId;
use krad_protocol::{AgentRef, RemoteBusState, RunnerBusShown};
use std::collections::HashMap;
use std::fmt;
use std::time::Instant;

use crate::hosts::{HostCliError, RemoteBus};

#[derive(Default)]
enum Slot {
    #[default]
    Absent,
    Preparing,
    Ready(RemoteBus),
}

impl Slot {
    fn shown(&self) -> RemoteBusState {
        match self {
            Self::Absent => RemoteBusState::Absent,
            Self::Preparing => RemoteBusState::Preparing,
            Self::Ready(bus) => RemoteBusState::Ready(bus.info().clone()),
        }
    }
}

#[derive(Default)]
struct Kept {
    slot: Slot,
    port: Option<u16>,
}

#[derive(Default)]
pub(crate) struct RunnerBuses {
    kept: HashMap<CloudRunnerId, Kept>,
    heard: HashMap<AgentRef, Instant>,
}

impl fmt::Debug for RunnerBuses {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RunnerBuses")
            .field("runners", &self.kept.len())
            .finish_non_exhaustive()
    }
}

fn shown(runner: CloudRunnerId, kept: &Kept) -> RunnerBusShown {
    RunnerBusShown {
        runner,
        bus: kept.slot.shown(),
    }
}

impl RunnerBuses {
    pub(crate) fn ready(&self, runner: CloudRunnerId) -> Option<&krad_protocol::RemoteBusInfo> {
        match &self.kept.get(&runner)?.slot {
            Slot::Ready(bus) => Some(bus.info()),
            Slot::Absent | Slot::Preparing => None,
        }
    }

    pub(crate) fn is_preparing(&self, runner: CloudRunnerId) -> bool {
        self.kept
            .get(&runner)
            .is_some_and(|kept| matches!(kept.slot, Slot::Preparing))
    }

    pub(crate) fn port(&self, runner: CloudRunnerId) -> Option<u16> {
        self.kept.get(&runner).and_then(|kept| kept.port)
    }

    pub(crate) fn preparing(&mut self, runner: CloudRunnerId) -> RunnerBusShown {
        let kept = self.kept.entry(runner).or_default();
        kept.slot = Slot::Preparing;
        shown(runner, kept)
    }

    pub(crate) fn prepared(
        &mut self,
        runner: CloudRunnerId,
        prepared: Option<Result<RemoteBus, HostCliError>>,
    ) -> RunnerBusShown {
        let kept = self.kept.entry(runner).or_default();
        kept.slot = match prepared {
            Some(Ok(bus)) => {
                kept.port = Some(bus.remote_port());
                Slot::Ready(bus)
            }
            Some(Err(error)) => {
                tracing::info!(%runner, %error, "the cloud runner's agent bus is unavailable");
                Slot::Absent
            }
            None => Slot::Absent,
        };
        shown(runner, kept)
    }

    pub(crate) fn drop_bus(&mut self, runner: CloudRunnerId) -> Option<RunnerBusShown> {
        let kept = self.kept.get_mut(&runner)?;
        if matches!(kept.slot, Slot::Absent) {
            return None;
        }
        kept.slot = Slot::Absent;
        Some(shown(runner, kept))
    }

    pub(crate) fn forget(&mut self, runner: CloudRunnerId) -> Option<RunnerBusShown> {
        let shown = self.drop_bus(runner);
        self.kept.remove(&runner);
        shown
    }

    pub(crate) fn lost(&mut self) -> Vec<RunnerBusShown> {
        self.kept
            .iter_mut()
            .filter_map(|(runner, kept)| {
                let Slot::Ready(bus) = &mut kept.slot else {
                    return None;
                };
                if bus.is_alive() {
                    return None;
                }
                kept.slot = Slot::Absent;
                Some(shown(*runner, kept))
            })
            .collect()
    }

    pub(crate) fn hear(&mut self, agent: AgentRef, at: Instant) {
        self.heard.insert(agent, at);
    }

    pub(crate) fn heard_since(&self, agent: &AgentRef, started: Instant) -> bool {
        self.heard.get(agent).is_some_and(|heard| *heard >= started)
    }

    pub(crate) fn snapshot(&self) -> Vec<RunnerBusShown> {
        let mut buses: Vec<RunnerBusShown> = self
            .kept
            .iter()
            .map(|(runner, kept)| shown(*runner, kept))
            .collect();
        buses.sort_by_key(|shown| shown.runner);
        buses
    }
}
