use krad_domain::{
    automation_since, AutomationEvent, AutomationKey, KraddieWake, KraddieWorkspace, LocalMoment,
};
use krad_protocol::{AutomationState, KraddieState};

use super::Kraddies;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WakeOrder {
    pub(crate) key: AutomationKey,
    pub(crate) wake: KraddieWake,
    pub(crate) prompt: Option<String>,
}

impl WakeOrder {
    pub(crate) const fn new(key: AutomationKey, wake: KraddieWake) -> Self {
        Self {
            key,
            wake,
            prompt: None,
        }
    }
}

fn routines(states: &[KraddieState]) -> impl Iterator<Item = (&KraddieState, &AutomationState)> {
    states
        .iter()
        .filter(|kraddie| kraddie.kraddie.switch.is_on())
        .flat_map(|kraddie| {
            kraddie
                .automations
                .iter()
                .map(move |state| (kraddie, state))
        })
}

fn key_of(kraddie: &KraddieState, state: &AutomationState) -> AutomationKey {
    AutomationKey::new(kraddie.id(), state.automation.id)
}

impl Kraddies {
    pub(crate) fn due(&self, now: LocalMoment) -> Vec<WakeOrder> {
        routines(&self.states)
            .filter(|(kraddie, state)| !self.limiter.is_running(&key_of(kraddie, state)))
            .filter(|(_, state)| {
                let since = automation_since(state.history.last_started(), self.opened_at);
                state.automation.is_due_at(since, now)
            })
            .map(|(kraddie, state)| WakeOrder::new(key_of(kraddie, state), KraddieWake::Schedule))
            .collect()
    }

    pub(crate) fn missed(&self) -> Vec<WakeOrder> {
        let opened = LocalMoment {
            at: self.opened_at,
            offset: self.now().offset,
        };
        routines(&self.states)
            .filter(|(_, state)| {
                state
                    .automation
                    .missed_run_at(state.history.last_started(), opened)
            })
            .map(|(kraddie, state)| WakeOrder::new(key_of(kraddie, state), KraddieWake::CatchUp))
            .collect()
    }

    pub(crate) fn woken_by(
        &self,
        place: &KraddieWorkspace,
        event: AutomationEvent,
    ) -> Vec<WakeOrder> {
        self.states
            .iter()
            .filter(|kraddie| kraddie.kraddie.works_in(place))
            .flat_map(|kraddie| {
                kraddie
                    .automations
                    .iter()
                    .map(move |state| (kraddie, state))
            })
            .filter(|(_, state)| state.automation.listens_for(event))
            .map(|(kraddie, state)| {
                WakeOrder::new(key_of(kraddie, state), KraddieWake::from(event))
            })
            .collect()
    }
}
