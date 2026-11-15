use gpui_kit::{App, Context};
use krad_domain::RuntimeId;
use krad_protocol::{EventKind, Snapshot, StatusReport, TerminalClassified, TerminalSummary};

use super::super::EnvironmentShell;
use super::view_model::{Area, Heard, News};
use super::HostMove;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Reach {
    Quietly,
    Shown,
    Attention,
    Terminals,
}

impl From<Area> for Reach {
    fn from(area: Area) -> Self {
        match area {
            Area::Attention | Area::Settings => Self::Attention,
            Area::Statuses
            | Area::Health
            | Area::Usage
            | Area::Models
            | Area::Opencode
            | Area::Hosts
            | Area::Runners => Self::Shown,
        }
    }
}

impl EnvironmentShell {
    pub(super) fn runtime_news(&mut self, cx: &mut Context<Self>) {
        let News { heard, areas } = self.runtime.view.update(cx, |model, _| model.take_news());
        self.runtime.reread(cx);
        self.environments_reread(cx);
        self.kraddies_redrawn();
        let shown = areas.iter().copied().map(Reach::from).max();
        let reach = heard
            .into_iter()
            .map(|heard| self.apply_heard(heard, cx))
            .fold(shown.unwrap_or(Reach::Quietly), Ord::max);
        if areas.contains(&Area::Health) {
            self.advance_harness_onboarding(cx);
        }
        if areas.contains(&Area::Runners) {
            self.reach_ready_runners(cx);
            self.follow_hosted_places(cx);
        }
        match reach {
            Reach::Quietly => {}
            Reach::Shown => cx.notify(),
            Reach::Attention => {
                self.flush_attention(cx);
                cx.notify();
            }
            Reach::Terminals => {
                self.flush_live_work(cx);
                cx.notify();
            }
        }
        let pace = self.frame_interval();
        self.runtime.view.update(cx, |model, _| model.pace = pace);
    }

    fn apply_heard(&mut self, heard: Heard, cx: &mut Context<Self>) -> Reach {
        match heard {
            Heard::Snapshot { snapshot, moves } => {
                self.snapshot_heard(*snapshot, moves, cx);
                return Reach::Terminals;
            }
            Heard::Event(kind) => return self.apply_runtime_event(kind, cx),
            Heard::HostMoved(moved) => self.host_moved(&moved, cx),
            Heard::RemoteBusMoved {
                environment,
                before,
            } => self.remote_bus_moved(&environment, &before, cx),
            Heard::AskHeld(pending) => self.hold_ask(pending),
            Heard::AskLetGo(ask) => {
                self.forget_held_ask(ask);
            }
            Heard::ModelsRelisted(harness) => {
                self.forget_unoffered_models(harness, cx);
                return Reach::Shown;
            }
            Heard::OpencodeReady => self.opencode_ready(),
            Heard::SettingsAdopted => self.settings_adopted(cx),
            Heard::GitMoved(moved) => self.git_moved(moved, cx),
            Heard::NodesMoved { environment, moved } => {
                self.nodes_moved(&environment, moved, cx);
                return Reach::Quietly;
            }
            Heard::HostedFailed { runner, failure } => {
                self.hosted_failed(runner, &failure, cx);
                return Reach::Shown;
            }
            Heard::RemoteChat { runner, kind } => {
                self.chat_event_heard(RuntimeId::Runner(runner), *kind, cx);
                return Reach::Shown;
            }
            Heard::RunnerOutdated(runner) => {
                self.runner_outdated(runner, cx);
                return Reach::Shown;
            }
            Heard::RemoteReplaced(runner) => {
                self.follow_hosted_places(cx);
                self.reseed_hosted_chats(runner, cx);
                return Reach::Shown;
            }
        }
        Reach::Attention
    }

    fn snapshot_heard(&mut self, snapshot: Snapshot, moves: Vec<HostMove>, cx: &mut Context<Self>) {
        self.gateway_replaced(cx);
        let environments = self.runtime.environments.names();
        for environment in environments {
            self.offer_open_buffers(&environment, cx);
        }
        self.advance_harness_onboarding(cx);
        self.hosts_replaced(&snapshot.hosts, moves, cx);
        self.runtime_chats_replaced(RuntimeId::Local, snapshot.chats, cx);
        for summary in snapshot.terminals {
            self.adopt_launched_view(&summary.terminal, cx);
            self.terminal_summary_heard(summary, cx);
        }
    }

    fn apply_runtime_event(&mut self, kind: EventKind, cx: &mut Context<Self>) -> Reach {
        match kind {
            EventKind::EnvironmentOpened { environment } => {
                self.offer_open_buffers(&environment.name, cx);
                Reach::Quietly
            }
            EventKind::EnvironmentChanged { environment, .. } => {
                self.environment_heard(&environment);
                Reach::Quietly
            }
            EventKind::EnvironmentClosed { .. } | EventKind::EnvironmentsOrdered { .. } => {
                Reach::Attention
            }
            EventKind::TerminalChanged { terminal } => {
                if terminal.environment == self.active {
                    Reach::Shown
                } else {
                    Reach::Quietly
                }
            }
            EventKind::TerminalEnded(summary) => {
                self.terminal_summary_heard(summary, cx);
                Reach::Terminals
            }
            EventKind::TerminalLaunched(launched) => {
                self.terminal_launched(launched, cx);
                Reach::Terminals
            }
            EventKind::TerminalClassified(TerminalClassified { terminal, state }) => {
                if self.terminal_classified(&terminal.environment, terminal.terminal, state, cx) {
                    Reach::Terminals
                } else {
                    Reach::Quietly
                }
            }
            chat if chat.chat().is_some() => {
                self.chat_event_heard(RuntimeId::Local, chat, cx);
                Reach::Attention
            }
            kind => {
                self.apply_shown_event(kind, cx);
                Reach::Attention
            }
        }
    }

    fn terminal_summary_heard(&mut self, summary: TerminalSummary, cx: &mut App) {
        let TerminalSummary { terminal, run } = summary;
        if !run.is_running() {
            self.terminal_ended(&terminal.environment, terminal.terminal, run, cx);
        }
    }

    fn apply_shown_event(&mut self, kind: EventKind, cx: &mut Context<Self>) {
        match kind {
            EventKind::AppEvent(record) => self.app_event_heard(&record),
            EventKind::ChatImageAttached { chat, image } => self.image_attached(&chat, image, cx),
            EventKind::Problem(problem) => self.problem_heard(problem),
            EventKind::UploadProgress(shown) => self.upload_progressed(shown, cx),
            EventKind::WebhookTested(tested) => self.webhook_tested(tested, cx),
            EventKind::AttentionRaised(notice) => self.attention_raised(notice),
            EventKind::AgentStatus(report) => self.agent_status_heard(&report),
            EventKind::AgentHooked(heard) => self.hook_heard(heard, cx),
            EventKind::ModelListFailed { error, .. } => self.model_list_failed(&error, cx),
            EventKind::AcpOffers(offers) => self.acp_offers_heard(&offers, cx),
            EventKind::OpencodeTerminalState { terminal, state } => {
                self.apply_opencode_state(&terminal.environment, terminal.terminal, state, cx);
            }
            gateway
            @ (EventKind::GatewayChanged { .. } | EventKind::GatewayKeyForgotten { .. }) => {
                self.gateway_heard(gateway, cx);
            }
            EventKind::SpendingCrossed(crossed) => self.spending_crossed(&crossed),
            EventKind::SettingsNotSaved { error } => self.settings_not_saved(&error),
            EventKind::GitFinished(finished) => self.git_finished(finished, cx),
            EventKind::SessionChangesRead(read) => self.session_changes_read(read),
            EventKind::WorktreeSwept(swept) => self.worktree_swept(&swept),
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
            EventKind::RunnerChanged(shown) => self.runner_changed(shown.runner, cx),
            EventKind::RunnerRemoved(removed) => self.runner_removed(removed, cx),
            EventKind::RunnerNews(notice) => self.runner_news(notice, cx),
            EventKind::CloudPatchOffered { draft } => self.cloud_patch_offered(draft, cx),
            EventKind::CloudNews(news) => self.cloud_news(news, cx),
            EventKind::CloudSessionOpened(opened) => self.cloud_session_opened(opened, cx),
            host @ (EventKind::SignInWanted { .. }
            | EventKind::MachineChannelWanted { .. }
            | EventKind::HostAgentsFound { .. }
            | EventKind::LocalAgentsFound(_)
            | EventKind::AcpAdapterChanged(_)) => self.host_event_heard(host, cx),
            EventKind::EnvironmentOpened { .. }
            | EventKind::EnvironmentClosed { .. }
            | EventKind::KraddieChanged { .. }
            | EventKind::DigestsChanged(_)
            | EventKind::AttentionLeft { .. }
            | EventKind::AttentionHistoryCleared
            | EventKind::UsageShown(_)
            | EventKind::AskPending(_)
            | EventKind::AskSettled { .. }
            | EventKind::HarnessChecked(_)
            | EventKind::ModelsListed { .. }
            | EventKind::OpencodeServer(_)
            | EventKind::SettingsChanged { .. }
            | EventKind::GitRead(_)
            | EventKind::KraddieRemoved { .. }
            | EventKind::HostChanged { .. }
            | EventKind::RemoteBusChanged { .. }
            | EventKind::HostPrompted(_)
            | EventKind::HostPromptSettled { .. }
            | EventKind::RunnerBusChanged(_)
            | EventKind::RunnerLoginsChanged(_)
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
            | EventKind::RunnerReturned { .. }
            | EventKind::Pong { .. }
            | EventKind::Answered { .. }
            | EventKind::BufferOpened(_)
            | EventKind::BufferEdited(_)
            | EventKind::BufferSaved { .. }
            | EventKind::BufferClosed { .. }
            | EventKind::BufferNotOpened(_)
            | EventKind::FolderChanged(_)
            | EventKind::Unknown => {}
        }
    }

    fn agent_status_heard(&mut self, report: &StatusReport) {
        let host = self.env_host(&report.agent.environment);
        self.statusline_heard(host, &report.status);
    }
}
