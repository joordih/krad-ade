use krad_protocol::{
    AskId, EventKind, ModelsOrigin, OpencodeStatus, PendingAsk, RunnerShown, Snapshot,
};
use std::collections::BTreeSet;
use std::mem::take;

use super::super::spot;
use super::{Area, Heard, ViewModel};

impl ViewModel {
    pub(in crate::environment_shell) fn adopt_snapshot(&mut self, mut snapshot: Snapshot) {
        self.statuses = take(&mut snapshot.bus.statuses)
            .into_iter()
            .map(|report| (spot(report.agent), report.status))
            .collect();
        self.mail.replace(take(&mut snapshot.bus.mail));
        self.attention.replace(take(&mut snapshot.attention));
        self.health.replace(take(&mut snapshot.health.checks));
        self.usage.replace(take(&mut snapshot.usage.shown));
        self.models.replace(take(&mut snapshot.models.listed));
        self.adapters = take(&mut snapshot.hosts.adapters)
            .into_iter()
            .map(|kept| (kept.agent, kept.adapter))
            .collect();
        self.opencode.replace(take(&mut snapshot.opencode));
        self.settings.replace(*take(&mut snapshot.settings));
        let runners = *take(&mut snapshot.runners);
        self.runners = runners
            .shown
            .into_iter()
            .map(|shown| (shown.runner, shown.state))
            .collect();
        self.forget_unready_remotes();
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
        self.adopt_asks(take(&mut snapshot.bus.asks));
        let prompts = take(&mut snapshot.hosts.prompts);
        let moves = self.hosts.replace(&snapshot.hosts.hosts, prompts);
        self.news.areas.extend(Area::ALL);
        self.heard(Heard::Snapshot {
            snapshot: Box::new(snapshot),
            moves,
        });
    }

    fn adopt_asks(&mut self, asks: Vec<PendingAsk>) {
        let pending: BTreeSet<AskId> = asks.iter().map(|pending| pending.ask).collect();
        let settled: Vec<AskId> = self.asks.difference(&pending).copied().collect();
        for ask in settled {
            self.asks.remove(&ask);
            self.heard(Heard::AskLetGo(ask));
        }
        for pending in asks {
            self.ask_pending(pending);
        }
    }

    fn ask_pending(&mut self, pending: PendingAsk) {
        if self.asks.insert(pending.ask) {
            self.heard(Heard::AskHeld(pending));
        }
    }

    pub(in crate::environment_shell) fn fold_event(&mut self, kind: EventKind) {
        let area = match kind {
            EventKind::Answered { request, answer } => {
                return self.waiters.answered(request, *answer);
            }
            EventKind::AttentionLeft { target } => {
                self.attention.left(&target);
                Area::Attention
            }
            EventKind::AttentionHistoryCleared => {
                self.attention.history_cleared();
                Area::Attention
            }
            EventKind::AttentionRaised(notice) => {
                self.attention.raised(&notice);
                self.heard(Heard::Event(EventKind::AttentionRaised(notice)));
                Area::Attention
            }
            EventKind::Mail(mail) => {
                self.mail.set(mail);
                Area::Mail
            }
            EventKind::UsageShown(shown) => {
                self.usage.apply(shown);
                Area::Usage
            }
            EventKind::AgentStatus(report) => {
                self.statuses
                    .insert(spot(report.agent.clone()), report.status.clone());
                self.heard(Heard::Event(EventKind::AgentStatus(report)));
                Area::Statuses
            }
            EventKind::AskPending(pending) => return self.ask_pending(pending),
            EventKind::AskSettled { ask, .. } => {
                if self.asks.remove(&ask) {
                    self.heard(Heard::AskLetGo(ask));
                }
                return;
            }
            EventKind::HarnessChecked(checked) => {
                self.health.apply(checked);
                Area::Health
            }
            EventKind::ModelsListed { listed, origin } => {
                let harness = listed.harness;
                self.models.apply(listed);
                if origin == ModelsOrigin::Listing {
                    self.heard(Heard::ModelsRelisted(harness));
                }
                Area::Models
            }
            EventKind::OpencodeServer(shown) => {
                let ready = shown.status == OpencodeStatus::Ready;
                self.opencode.apply(shown);
                if ready {
                    self.heard(Heard::OpencodeReady);
                }
                Area::Opencode
            }
            EventKind::SettingsChanged { settings } => {
                if let Some(adopted) = self.settings.changed(*settings) {
                    self.heard(Heard::SettingsAdopted(Box::new(adopted)));
                }
                Area::Settings
            }
            kind => return self.fold_placed(kind),
        };
        self.changed(area);
    }

    fn fold_placed(&mut self, kind: EventKind) {
        let area = match kind {
            EventKind::RunnerChanged(RunnerShown { runner, state }) => {
                self.runners.insert(runner, state.clone());
                self.forget_unready_remotes();
                self.heard(Heard::Event(EventKind::RunnerChanged(RunnerShown {
                    runner,
                    state,
                })));
                Area::Runners
            }
            EventKind::RunnerBusChanged(shown) => {
                self.runner_buses.insert(shown.runner, shown.bus);
                Area::Runners
            }
            EventKind::RunnerLoginsChanged(logins) => {
                self.runner_logins.insert(logins.runner, logins);
                Area::Runners
            }
            EventKind::HostChanged { environment, state } => {
                let moved = self.hosts.changed(environment, state);
                self.heard(Heard::HostMoved(moved));
                Area::Hosts
            }
            EventKind::RemoteBusChanged { environment, bus } => {
                let before = self.hosts.bus_changed(environment.clone(), bus);
                self.heard(Heard::RemoteBusMoved {
                    environment,
                    before,
                });
                Area::Hosts
            }
            EventKind::AcpOffers(offers) => {
                self.acp.heard(&offers);
                self.heard(Heard::Event(EventKind::AcpOffers(offers)));
                Area::Models
            }
            EventKind::AcpAdapterChanged(kept) => {
                self.adapters.insert(kept.agent, kept.adapter.clone());
                self.heard(Heard::Event(EventKind::AcpAdapterChanged(kept)));
                Area::Hosts
            }
            EventKind::HostPrompted(prompt) => {
                self.hosts.prompted(prompt);
                Area::Hosts
            }
            EventKind::HostPromptSettled { prompt } => {
                self.hosts.prompt_settled(prompt);
                Area::Hosts
            }
            kind => return self.heard(Heard::Event(kind)),
        };
        self.changed(area);
    }
}
