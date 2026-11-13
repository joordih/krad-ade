use krad_domain::{EnvironmentName, Host};
use krad_host::HostLink;
use krad_protocol::{EventKind, RemoteBusState};
use std::fmt;
use std::sync::Arc;

use super::{Hosts, RemoteBus, RemoteBusPlan};
use crate::jobs::{JobDone, Jobs};

pub(super) enum BusSlot {
    Absent,
    Preparing,
    Ready(RemoteBus),
    Unavailable(String),
}

impl fmt::Debug for BusSlot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.shown(), formatter)
    }
}

impl BusSlot {
    pub(super) fn shown(&self) -> RemoteBusState {
        match self {
            Self::Absent => RemoteBusState::Absent,
            Self::Preparing => RemoteBusState::Preparing,
            Self::Ready(bus) => RemoteBusState::Ready(bus.info().clone()),
            Self::Unavailable(reason) => RemoteBusState::Unavailable {
                reason: reason.clone(),
            },
        }
    }
}

impl Hosts {
    pub(super) fn set_bus(
        &mut self,
        environment: &EnvironmentName,
        bus: BusSlot,
        outbox: &mut Vec<EventKind>,
    ) {
        let Some(entry) = self.envs.get_mut(environment) else {
            return;
        };
        if let BusSlot::Ready(ready) = &bus {
            entry.bus_port = Some(ready.remote_port());
        }
        let shown = bus.shown();
        entry.bus = bus;
        outbox.push(EventKind::RemoteBusChanged {
            environment: environment.clone(),
            bus: shown,
        });
    }

    pub(super) fn prepare_bus(
        &mut self,
        environment: &EnvironmentName,
        jobs: &Jobs,
        outbox: &mut Vec<EventKind>,
    ) {
        let Some(entry) = self.envs.get(environment) else {
            return;
        };
        let (host, port) = (entry.host.clone(), entry.bus_port);
        let link = self.links.link(&host);
        let plan = match (&self.bus, &link) {
            (Some(bus), HostLink::Remote(ssh)) => RemoteBusPlan::new(
                Arc::clone(ssh),
                link.clone(),
                environment.as_str(),
                &bus.endpoint,
                port,
            ),
            _ => return self.set_bus(environment, BusSlot::Absent, outbox),
        };
        self.set_bus(environment, BusSlot::Preparing, outbox);
        let job_environment = environment.clone();
        let started = jobs.run(move || JobDone::RemoteBusPrepared {
            environment: job_environment,
            host,
            prepared: plan.prepare().map_err(|error| error.to_string()),
        });
        if let Err(error) = started {
            self.set_bus(environment, BusSlot::Unavailable(error.to_string()), outbox);
        }
    }

    pub(crate) fn bus_prepared(
        &mut self,
        environment: &EnvironmentName,
        host: &Host,
        prepared: Result<RemoteBus, String>,
        outbox: &mut Vec<EventKind>,
    ) {
        let current = self
            .envs
            .get(environment)
            .is_some_and(|entry| entry.host == *host && matches!(entry.bus, BusSlot::Preparing));
        if !current {
            return;
        }
        let slot = match prepared {
            Ok(bus) => BusSlot::Ready(bus),
            Err(reason) => {
                tracing::info!(environment = environment.as_str(), %reason, "remote agent bus unavailable");
                BusSlot::Unavailable(reason)
            }
        };
        self.set_bus(environment, slot, outbox);
    }

    pub(super) fn drop_lost_buses(&mut self, outbox: &mut Vec<EventKind>) {
        let lost_buses: Vec<EnvironmentName> = self
            .envs
            .iter_mut()
            .filter_map(|(name, entry)| match &mut entry.bus {
                BusSlot::Ready(bus) => (!bus.is_alive()).then(|| name.clone()),
                BusSlot::Absent | BusSlot::Preparing | BusSlot::Unavailable(_) => None,
            })
            .collect();
        for environment in lost_buses {
            self.set_bus(&environment, BusSlot::Absent, outbox);
        }
    }
}
