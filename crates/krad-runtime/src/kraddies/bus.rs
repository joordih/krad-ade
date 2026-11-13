use krad_agents::{AutomationInfo, ErrorCode, KraddieInfo, Response};
use krad_domain::{
    automation_since, local_stamp, schedule_label, trigger_label, AcpSettings, AutomationKey,
    AutomationQuery, LocalOffset, RunRefusal,
};
use krad_protocol::{AutomationState, KraddieState};
use std::fmt;
use std::time::SystemTime;

use super::Kraddies;

const AMBIGUOUS: &str = "Several Kraddies have an automation of that name; write <kraddie>/<name>";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum KraddieWakeError {
    Unknown,
    Off,
    NoWorkspace,
    EnvironmentClosed,
    GroupClosed,
    NotAChatAgent,
    NoAutomations,
    Refused(RunRefusal),
}

impl fmt::Display for KraddieWakeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown => f.write_str("No automation with that name"),
            Self::Off => f.write_str("The automation or its Kraddie is switched off"),
            Self::NoWorkspace => f.write_str(
                "The Kraddie has no repository to work in; edit it in the Kraddies panel",
            ),
            Self::EnvironmentClosed => f.write_str("The Kraddie's environment is not open"),
            Self::GroupClosed => f.write_str("The automation's sidebar group is not open"),
            Self::NotAChatAgent => {
                f.write_str("The automation's agent cannot run as a worker chat")
            }
            Self::NoAutomations => {
                f.write_str("The Kraddie has no automations; add one in the Automations view")
            }
            Self::Refused(refusal) => refusal.fmt(f),
        }
    }
}

impl KraddieWakeError {
    pub(crate) const fn code(&self) -> ErrorCode {
        match self {
            Self::Unknown | Self::NoAutomations => ErrorCode::NotFound,
            Self::Off => ErrorCode::Forbidden,
            Self::NoWorkspace | Self::NotAChatAgent => ErrorCode::Unsupported,
            Self::EnvironmentClosed | Self::GroupClosed | Self::Refused(_) => {
                ErrorCode::Unavailable
            }
        }
    }

    pub(crate) const fn reply_code(&self) -> krad_protocol::ErrorCode {
        match self {
            Self::Unknown | Self::NoAutomations => krad_protocol::ErrorCode::NotFound,
            Self::Off => krad_protocol::ErrorCode::Invalid,
            Self::NoWorkspace | Self::NotAChatAgent => krad_protocol::ErrorCode::Unsupported,
            Self::EnvironmentClosed | Self::GroupClosed | Self::Refused(_) => {
                krad_protocol::ErrorCode::Unavailable
            }
        }
    }
}

fn next_run(
    kraddie: &KraddieState,
    state: &AutomationState,
    opened_at: SystemTime,
    offset: LocalOffset,
) -> Option<SystemTime> {
    if !kraddie.kraddie.switch.is_on() || !state.automation.switch.is_on() {
        return None;
    }
    let since = automation_since(state.history.last_started(), opened_at);
    state.automation.next_run_at(since, offset)
}

pub(crate) fn automation_info(
    kraddies: &Kraddies,
    kraddie: &KraddieState,
    state: &AutomationState,
) -> AutomationInfo {
    let offset = kraddies.now().offset;
    AutomationInfo {
        kraddie: kraddie.id().to_string(),
        id: state.automation.id.0,
        name: state.automation.name.to_string(),
        switch: state.automation.switch,
        triggers: state
            .automation
            .triggers
            .iter()
            .map(|trigger| trigger_label(*trigger))
            .collect(),
        next_run: next_run(kraddie, state, kraddies.opened_at(), offset)
            .map(|at| local_stamp(at, offset)),
        last_outcome: state
            .history
            .latest()
            .map(|run| run.outcome.label().to_owned()),
        runs: state.history.len(),
    }
}

pub(crate) fn kraddie_infos(kraddies: &Kraddies, acp: &AcpSettings) -> Vec<KraddieInfo> {
    let offset = kraddies.now().offset;
    kraddies
        .states()
        .iter()
        .map(|state| {
            let default = state.default_automation();
            KraddieInfo {
                id: state.id().to_string(),
                name: state.kraddie.name.to_string(),
                switch: state.kraddie.switch,
                harness: acp.harness_name(state.kraddie.harness).into_owned(),
                schedule: default
                    .and_then(|routine| routine.automation.schedules().next().copied())
                    .map(schedule_label),
                next_run: default
                    .and_then(|routine| next_run(state, routine, kraddies.opened_at(), offset))
                    .map(|at| local_stamp(at, offset)),
                last_outcome: state
                    .automations
                    .iter()
                    .filter_map(|routine| routine.history.latest())
                    .max_by_key(|run| run.started)
                    .map(|run| run.outcome.label().to_owned()),
            }
        })
        .collect()
}

impl Kraddies {
    pub(crate) fn automation_infos(&self) -> Vec<AutomationInfo> {
        self.states()
            .iter()
            .flat_map(|kraddie| {
                kraddie
                    .automations
                    .iter()
                    .map(move |state| automation_info(self, kraddie, state))
            })
            .collect()
    }

    pub(crate) fn find_automation(&self, asked: &str) -> Result<AutomationKey, Response> {
        let query = AutomationQuery::from(asked);
        let found: Vec<AutomationKey> = self
            .states()
            .iter()
            .filter(|kraddie| {
                query
                    .kraddie
                    .is_none_or(|name| kraddie.kraddie.name.matches(name))
            })
            .flat_map(|kraddie| {
                kraddie
                    .automations
                    .iter()
                    .filter(|state| state.automation.name.matches(query.name))
                    .map(|state| AutomationKey::new(kraddie.id(), state.automation.id))
            })
            .collect();
        match found.as_slice() {
            [key] => Ok(key.clone()),
            [] => Err(Response::error(
                ErrorCode::NotFound,
                format!("No automation named {asked}"),
            )),
            _ => Err(Response::error(ErrorCode::BadRequest, AMBIGUOUS)),
        }
    }

    pub(crate) fn default_key(&self, kraddie: &krad_domain::KraddieId) -> Option<AutomationKey> {
        let state = self.state(kraddie)?;
        let routine = state.default_automation()?;
        Some(AutomationKey::new(state.id(), routine.automation.id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refusals_keep_stable_error_codes() {
        assert_eq!(KraddieWakeError::Unknown.code(), ErrorCode::NotFound);
        assert_eq!(KraddieWakeError::Off.code(), ErrorCode::Forbidden);
        assert_eq!(KraddieWakeError::NoWorkspace.code(), ErrorCode::Unsupported);
        assert_eq!(KraddieWakeError::GroupClosed.code(), ErrorCode::Unavailable);
        assert_eq!(
            KraddieWakeError::Refused(RunRefusal::AtCapacity).code(),
            ErrorCode::Unavailable
        );
    }
}
