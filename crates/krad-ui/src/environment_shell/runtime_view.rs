use futures::{FutureExt as _, StreamExt as _};
use gpui_kit::Context;
use krad_domain::{AgentStatus, CloudRunnerId, EnvironmentName, RunnerState, TerminalId};
use krad_protocol::{
    AgentRef, AskId, BusSnapshot, EventKind, Feed, RemoteBusState, StatusReport,
    TerminalClassified, TerminalSummary,
};
use krad_runtime::RuntimeHandle;
use krad_session::AnsiPalette;
use std::collections::{BTreeSet, HashMap};

mod environments;
mod hosts;
mod mail;
mod settings;

pub(super) use environments::EnvironmentsMirror;
pub(super) use hosts::{HostMove, HostsMirror};
pub(super) use mail::MailNotices;
use settings::SettingsMirror;

use super::attention::AttentionMirror;
use super::harness_health::HealthMirror;
use super::harness_models::ModelsMirror;
use super::opencode::OpencodeMirror;
use super::usage_bar::UsageMirror;
use super::EnvironmentShell;

type AgentSpot = (EnvironmentName, TerminalId);

fn spot(agent: AgentRef) -> AgentSpot {
    (agent.environment, agent.terminal)
}

pub(super) struct RuntimeLink {
    pub(super) handle: RuntimeHandle,
    pub(super) view: RuntimeView,
    palette: Option<AnsiPalette>,
}

impl RuntimeLink {
    pub(super) fn new(handle: RuntimeHandle) -> Self {
        Self {
            handle,
            view: RuntimeView::default(),
            palette: None,
        }
    }

    pub(super) fn use_palette(&mut self, palette: AnsiPalette) {
        if self.palette.replace(palette) != Some(palette) {
            self.handle.terminal_palette(palette);
        }
    }
}

#[derive(Default)]
pub(super) struct RuntimeView {
    statuses: HashMap<AgentSpot, AgentStatus>,
    asks: BTreeSet<AskId>,
    pub(super) mail: MailNotices,
    pub(super) attention: AttentionMirror,
    pub(super) health: HealthMirror,
    pub(super) usage: UsageMirror,
    pub(super) models: ModelsMirror,
    pub(super) opencode: OpencodeMirror,
    pub(super) environments: EnvironmentsMirror,
    pub(super) hosts: HostsMirror,
    pub(super) settings: SettingsMirror,
    pub(super) runners: HashMap<CloudRunnerId, RunnerState>,
    pub(super) runner_buses: HashMap<CloudRunnerId, RemoteBusState>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Heard {
    Quietly,
    Visibly,
    FromTerminals,
}

impl EnvironmentShell {
    pub(super) fn follow_runtime(&mut self, cx: &mut Context<Self>) {
        let mut feed = self.runtime.handle.subscribe();
        cx.spawn(async move |this, cx| {
            while let Some(first) = feed.next().await {
                let mut batch = vec![first];
                while let Some(Some(more)) = feed.next().now_or_never() {
                    batch.push(more);
                }
                let Ok(interval) = this.update(cx, |shell, cx| {
                    shell.apply_runtime_feeds(batch, cx);
                    shell.frame_interval()
                }) else {
                    break;
                };
                cx.background_executor().timer(interval).await;
            }
        })
        .detach();
    }

    pub(super) fn agent_status(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
    ) -> Option<&AgentStatus> {
        self.runtime.view.statuses.get(&(env.clone(), id))
    }

    fn apply_runtime_feeds(&mut self, batch: Vec<Feed>, cx: &mut Context<Self>) {
        let heard = batch
            .into_iter()
            .map(|feed| self.apply_runtime_feed(feed, cx))
            .max()
            .unwrap_or(Heard::Quietly);
        match heard {
            Heard::Quietly => return,
            Heard::Visibly => self.flush_attention(cx),
            Heard::FromTerminals => self.flush_live_work(cx),
        }
        cx.notify();
    }

    fn apply_runtime_feed(&mut self, feed: Feed, cx: &mut Context<Self>) -> Heard {
        match feed {
            Feed::Snapshot(snapshot) => {
                self.apply_bus_snapshot(snapshot.bus);
                self.runtime.view.attention.replace(snapshot.attention);
                self.runtime.view.health.replace(snapshot.health.checks);
                self.runtime.view.usage.replace(snapshot.usage.shown);
                self.runtime.view.models.replace(snapshot.models.listed);
                self.runtime.view.opencode.replace(snapshot.opencode);
                self.gateway_replaced(snapshot.gateway);
                self.runtime.view.settings.replace(*snapshot.settings);
                self.runtime.view.runners = snapshot
                    .runners
                    .into_iter()
                    .map(|shown| (shown.runner, shown.state))
                    .collect();
                self.runtime.view.runner_buses = snapshot
                    .runner_buses
                    .into_iter()
                    .map(|shown| (shown.runner, shown.bus))
                    .collect();
                for environment in &snapshot.environments {
                    self.offer_open_buffers(&environment.name, cx);
                }
                self.runtime
                    .view
                    .environments
                    .replace(snapshot.environments);
                self.adopt_runtime_environments(cx);
                self.advance_harness_onboarding(cx);
                self.hosts_replaced(*snapshot.hosts, cx);
                self.runtime_chats_replaced(snapshot.chats, cx);
                self.git_replaced(snapshot.git, cx);
                self.kraddies_replaced(*snapshot.kraddies);
                for summary in snapshot.terminals {
                    self.adopt_launched_view(&summary.terminal);
                    self.terminal_summary_heard(summary);
                }
                Heard::FromTerminals
            }
            Feed::Event(event) => self.apply_runtime_event(event.kind, cx),
        }
    }

    fn apply_runtime_event(&mut self, kind: EventKind, cx: &mut Context<Self>) -> Heard {
        let environments = &mut self.runtime.view.environments;
        match kind {
            EventKind::EnvironmentOpened { environment } => {
                let name = environment.name.clone();
                environments.opened(*environment);
                self.offer_open_buffers(&name, cx);
            }
            EventKind::EnvironmentClosed { environment } => environments.closed(&environment),
            EventKind::EnvironmentsOrdered { order } => environments.ordered(&order),
            EventKind::EnvironmentChanged {
                environment,
                changes,
                answers,
            } => {
                self.runtime
                    .view
                    .environments
                    .changed(&environment, changes.clone());
                self.environment_heard(&environment, &changes, answers, cx);
            }
            EventKind::TerminalChanged { terminal } => {
                return if terminal.environment == self.active {
                    Heard::Visibly
                } else {
                    Heard::Quietly
                };
            }
            EventKind::TerminalEnded(summary) => {
                self.terminal_summary_heard(summary);
                return Heard::FromTerminals;
            }
            EventKind::TerminalLaunched(launched) => {
                self.terminal_launched(launched);
                return Heard::FromTerminals;
            }
            EventKind::TerminalClassified(TerminalClassified { terminal, state }) => {
                return if self.terminal_classified(&terminal.environment, terminal.terminal, state)
                {
                    Heard::FromTerminals
                } else {
                    Heard::Quietly
                };
            }
            chat if chat.chat().is_some() => {
                self.chat_event_heard(chat, cx);
                return Heard::Visibly;
            }
            kind => {
                self.apply_shown_event(kind, cx);
                return Heard::Visibly;
            }
        }
        Heard::Quietly
    }

    fn terminal_summary_heard(&mut self, summary: TerminalSummary) {
        let TerminalSummary { terminal, run } = summary;
        if !run.is_running() {
            self.terminal_ended(&terminal.environment, terminal.terminal, run);
        }
    }

    fn apply_shown_event(&mut self, kind: EventKind, cx: &mut Context<Self>) {
        match kind {
            EventKind::AppEvent(record) => self.app_event_heard(&record),
            EventKind::WebhookTested(tested) => self.webhook_tested(tested, cx),
            EventKind::AttentionRaised(notice) => self.attention_raised(notice),
            EventKind::AttentionLeft { target } => self.runtime.view.attention.left(&target),
            EventKind::AttentionHistoryCleared => self.runtime.view.attention.history_cleared(),
            EventKind::AgentStatus(report) => self.agent_status_heard(report),
            EventKind::AgentHooked(heard) => self.hook_heard(heard),
            EventKind::Mail(mail) => self.runtime.view.mail.set(mail),
            EventKind::AskPending(pending) => {
                if self.runtime.view.asks.insert(pending.ask) {
                    self.hold_ask(pending);
                }
            }
            EventKind::AskSettled { ask, .. } => {
                if self.runtime.view.asks.remove(&ask) {
                    self.forget_held_ask(ask);
                }
            }
            EventKind::HarnessChecked(checked) => {
                self.runtime.view.health.apply(checked);
                self.advance_harness_onboarding(cx);
            }
            EventKind::UsageShown(shown) => self.runtime.view.usage.apply(shown),
            EventKind::ModelsListed { listed, origin } => self.models_listed(listed, origin, cx),
            EventKind::ModelListFailed { error, .. } => self.model_list_failed(&error, cx),
            EventKind::OpencodeServer(shown) => self.opencode_heard(shown, cx),
            gateway
            @ (EventKind::GatewayChanged { .. } | EventKind::GatewayKeyForgotten { .. }) => {
                self.gateway_heard(gateway, cx);
            }
            EventKind::SpendingCrossed(crossed) => self.spending_crossed(&crossed),
            EventKind::SettingsChanged { settings } => self.settings_heard(*settings, cx),
            EventKind::SettingsNotSaved { error } => self.settings_not_saved(&error),
            EventKind::GitRead(read) => self.git_read_heard(read, cx),
            EventKind::GitFinished(finished) => self.git_finished(finished, cx),
            EventKind::SessionChangesRead(read) => self.session_changes_read(read),
            EventKind::WorktreeSwept(swept) => self.worktree_swept(&swept),
            EventKind::KraddieChanged { kraddie } => self.kraddie_heard(*kraddie),
            EventKind::KraddieRemoved { kraddie } => self.kraddie_removed(&kraddie),
            EventKind::DigestsChanged(owned) => self.digests_heard(owned),
            EventKind::KraddieRunFailed { task, reason } => {
                self.kraddie_run_failed(&task, &reason, cx);
            }
            EventKind::KraddieNotSaved { error } => self.kraddie_not_saved(&error, cx),
            EventKind::ReviewAnswered(answered) => {
                let answered = *answered;
                let place = (answered.environment, answered.root);
                self.review_answered(answered.request, place, answered.answer, cx);
            }
            EventKind::CodeHostLogin(login) => self.code_host_login_heard(login, cx),
            EventKind::RunnerChanged(shown) => self.runner_changed(shown, cx),
            EventKind::RunnerRemoved(removed) => self.runner_removed(removed, cx),
            EventKind::RunnerBusChanged(shown) => {
                self.runtime
                    .view
                    .runner_buses
                    .insert(shown.runner, shown.bus);
            }
            EventKind::RunnerReturned { .. } => self.resume_cloud_work(cx),
            EventKind::CloudAskReplayed(asked) => self.cloud_ask_replayed(asked),
            host @ (EventKind::HostChanged { .. }
            | EventKind::SignInWanted { .. }
            | EventKind::MachineChannelWanted { .. }
            | EventKind::RemoteBusChanged { .. }
            | EventKind::HostPrompted(_)
            | EventKind::HostPromptSettled { .. }
            | EventKind::HostAgentsFound { .. }
            | EventKind::LocalAgentsFound(_)
            | EventKind::AcpAdapterChanged(_)) => self.host_event_heard(host, cx),
            EventKind::EnvironmentOpened { .. }
            | EventKind::EnvironmentClosed { .. }
            | EventKind::EnvironmentsOrdered { .. }
            | EventKind::EnvironmentChanged { .. }
            | EventKind::TerminalChanged { .. }
            | EventKind::TerminalEnded(_)
            | EventKind::TerminalLaunched(_)
            | EventKind::TerminalClassified(_)
            | EventKind::ChatOpened { .. }
            | EventKind::ChatMessageAdded { .. }
            | EventKind::ChatMessageChanged { .. }
            | EventKind::ChatMessageAppended { .. }
            | EventKind::ChatMessagesCut { .. }
            | EventKind::ChatMessagesPrepended { .. }
            | EventKind::ChatCheckpointed { .. }
            | EventKind::ChatState { .. }
            | EventKind::ChatAsk(_)
            | EventKind::ChatAskSettled { .. }
            | EventKind::ChatClosed { .. }
            | EventKind::ChatSpent { .. }
            | EventKind::Pong { .. }
            | EventKind::Unknown => {}
        }
    }

    fn agent_status_heard(&mut self, report: StatusReport) {
        let host = self.env_link(&report.agent.environment).host();
        self.statusline_heard(host, &report.status);
        self.runtime
            .view
            .statuses
            .insert(spot(report.agent), report.status);
    }

    fn apply_bus_snapshot(&mut self, bus: BusSnapshot) {
        self.runtime.view.statuses = bus
            .statuses
            .into_iter()
            .map(|report| (spot(report.agent), report.status))
            .collect();
        self.runtime.view.mail.replace(bus.mail);
        let pending: BTreeSet<AskId> = bus.asks.iter().map(|pending| pending.ask).collect();
        let settled: Vec<AskId> = self
            .runtime
            .view
            .asks
            .difference(&pending)
            .copied()
            .collect();
        for ask in settled {
            self.runtime.view.asks.remove(&ask);
            self.forget_held_ask(ask);
        }
        for pending in bus.asks {
            if self.runtime.view.asks.insert(pending.ask) {
                self.hold_ask(pending);
            }
        }
    }
}
