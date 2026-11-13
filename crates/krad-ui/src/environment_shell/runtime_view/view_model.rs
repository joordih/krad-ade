use krad_domain::{AgentStatus, CloudRunnerId, RunnerState};
use krad_protocol::{AskId, EventKind, RemoteBusState, RunnerLogins, Snapshot};
use std::collections::{BTreeSet, HashMap};
use std::mem::take;

use super::super::attention::AttentionMirror;
use super::super::harness_health::HealthMirror;
use super::super::harness_models::ModelsMirror;
use super::super::opencode::OpencodeMirror;
use super::super::usage_bar::UsageMirror;
use super::settings::SettingsMirror;
use super::{spot, AgentSpot, EnvironmentsMirror, HostsMirror, MailNotices};

#[derive(Default)]
pub(in crate::environment_shell) struct ViewModel {
    pub(in crate::environment_shell) statuses: HashMap<AgentSpot, AgentStatus>,
    pub(in crate::environment_shell) asks: BTreeSet<AskId>,
    pub(in crate::environment_shell) mail: MailNotices,
    pub(in crate::environment_shell) attention: AttentionMirror,
    pub(in crate::environment_shell) health: HealthMirror,
    pub(in crate::environment_shell) usage: UsageMirror,
    pub(in crate::environment_shell) models: ModelsMirror,
    pub(in crate::environment_shell) opencode: OpencodeMirror,
    pub(in crate::environment_shell) environments: EnvironmentsMirror,
    pub(in crate::environment_shell) hosts: HostsMirror,
    pub(in crate::environment_shell) settings: SettingsMirror,
    pub(in crate::environment_shell) runners: HashMap<CloudRunnerId, RunnerState>,
    pub(in crate::environment_shell) runner_buses: HashMap<CloudRunnerId, RemoteBusState>,
    pub(in crate::environment_shell) runner_logins: HashMap<CloudRunnerId, RunnerLogins>,
}

impl ViewModel {
    pub(in crate::environment_shell) fn adopt_snapshot(&mut self, snapshot: &mut Snapshot) {
        self.statuses = take(&mut snapshot.bus.statuses)
            .into_iter()
            .map(|report| (spot(report.agent), report.status))
            .collect();
        self.mail.replace(take(&mut snapshot.bus.mail));
        self.attention.replace(take(&mut snapshot.attention));
        self.health.replace(take(&mut snapshot.health.checks));
        self.usage.replace(take(&mut snapshot.usage.shown));
        self.models.replace(take(&mut snapshot.models.listed));
        self.opencode.replace(take(&mut snapshot.opencode));
        self.settings.replace(*take(&mut snapshot.settings));
        let runners = *take(&mut snapshot.runners);
        self.runners = runners
            .shown
            .into_iter()
            .map(|shown| (shown.runner, shown.state))
            .collect();
        self.runner_buses = runners
            .buses
            .into_iter()
            .map(|shown| (shown.runner, shown.bus))
            .collect();
        self.runner_logins = runners
            .logins
            .into_iter()
            .map(|shown| (shown.runner, shown))
            .collect();
        self.environments.replace(take(&mut snapshot.environments));
    }

    pub(in crate::environment_shell) fn fold_event(
        &mut self,
        kind: EventKind,
    ) -> Option<EventKind> {
        match kind {
            EventKind::AttentionLeft { target } => self.attention.left(&target),
            EventKind::AttentionHistoryCleared => self.attention.history_cleared(),
            EventKind::Mail(mail) => self.mail.set(mail),
            EventKind::UsageShown(shown) => self.usage.apply(shown),
            EventKind::RunnerBusChanged(shown) => {
                self.runner_buses.insert(shown.runner, shown.bus);
            }
            EventKind::RunnerLoginsChanged(logins) => {
                self.runner_logins.insert(logins.runner, logins);
            }
            EventKind::EnvironmentClosed { environment } => self.environments.closed(&environment),
            EventKind::EnvironmentsOrdered { order } => self.environments.ordered(&order),
            kind => return Some(kind),
        }
        None
    }
}

#[cfg(test)]
mod tests;
