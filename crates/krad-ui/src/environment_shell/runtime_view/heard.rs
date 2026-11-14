use gpui_kit::Context;
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
            | Area::Mail
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
            Heard::SettingsAdopted(settings) => self.settings_adopted(*settings, cx),
            Heard::HostedFailed { runner, failure } => {
                self.hosted_failed(runner, &failure, cx);
                return Reach::Shown;
            }
        }
        Reach::Attention
    }

    fn snapshot_heard(&mut self, snapshot: Snapshot, moves: Vec<HostMove>, cx: &mut Context<Self>) {
        self.runtime.environments.replace(snapshot.environments);
        self.gateway_replaced(snapshot.gateway);
        let environments = self.runtime.environments.names();
        for environment in environments {
            self.offer_open_buffers(&environment, cx);
        }
        self.adopt_runtime_environments(cx);
        self.advance_harness_onboarding(cx);
        self.hosts_replaced(*snapshot.hosts, moves, cx);
        self.runtime_chats_replaced(snapshot.chats, cx);
        self.git_replaced(snapshot.git, cx);
        self.kraddies_replaced(*snapshot.kraddies);
        for summary in snapshot.terminals {
            self.adopt_launched_view(&summary.terminal);
            self.terminal_summary_heard(summary);
        }
    }

    fn apply_runtime_event(&mut self, kind: EventKind, cx: &mut Context<Self>) -> Reach {
        match kind {
            EventKind::EnvironmentOpened { environment } => {
                let name = environment.name.clone();
                self.runtime.environments.opened(*environment);
                self.offer_open_buffers(&name, cx);
                Reach::Quietly
            }
            EventKind::EnvironmentChanged {
                environment,
                changes,
                answers,
            } => {
                self.runtime
                    .environments
                    .changed(&environment, changes.clone());
                self.environment_heard(&environment, &changes, answers, cx);
                Reach::Quietly
            }
            EventKind::EnvironmentClosed { environment } => {
                self.runtime.environments.closed(&environment);
                Reach::Attention
            }
            EventKind::EnvironmentsOrdered { order } => {
                self.runtime.environments.ordered(&order);
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
                self.terminal_summary_heard(summary);
                Reach::Terminals
            }
            EventKind::TerminalLaunched(launched) => {
                self.terminal_launched(launched);
                Reach::Terminals
            }
            EventKind::TerminalClassified(TerminalClassified { terminal, state }) => {
                if self.terminal_classified(&terminal.environment, terminal.terminal, state) {
                    Reach::Terminals
                } else {
                    Reach::Quietly
                }
            }
            chat if chat.chat().is_some() => {
                self.chat_event_heard(chat, cx);
                Reach::Attention
            }
            kind => {
                self.apply_shown_event(kind, cx);
                Reach::Attention
            }
        }
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
            EventKind::ChatImageAttached { chat, image } => self.image_attached(&chat, image, cx),
            EventKind::Problem(problem) => self.problem_heard(problem),
            EventKind::UploadProgress(shown) => self.upload_progressed(shown, cx),
            EventKind::WebhookTested(tested) => self.webhook_tested(tested, cx),
            EventKind::AttentionRaised(notice) => self.attention_raised(notice),
            EventKind::AgentStatus(report) => self.agent_status_heard(&report),
            EventKind::AgentHooked(heard) => self.hook_heard(heard),
            EventKind::ModelListFailed { error, .. } => self.model_list_failed(&error, cx),
            EventKind::AcpOffers(offers) => self.acp_offers_heard(offers, cx),
            EventKind::OpencodeTerminalState { terminal, state } => {
                self.apply_opencode_state(&terminal.environment, terminal.terminal, state);
            }
            gateway
            @ (EventKind::GatewayChanged { .. } | EventKind::GatewayKeyForgotten { .. }) => {
                self.gateway_heard(gateway, cx);
            }
            EventKind::SpendingCrossed(crossed) => self.spending_crossed(&crossed),
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
            EventKind::RunnerChanged(shown) => self.runner_changed(shown.runner, cx),
            EventKind::RunnerRemoved(removed) => self.runner_removed(removed, cx),
            EventKind::CloudAskReplayed(asked) => self.cloud_ask_replayed(asked),
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
            | EventKind::AttentionLeft { .. }
            | EventKind::AttentionHistoryCleared
            | EventKind::Mail(_)
            | EventKind::UsageShown(_)
            | EventKind::AskPending(_)
            | EventKind::AskSettled { .. }
            | EventKind::HarnessChecked(_)
            | EventKind::ModelsListed { .. }
            | EventKind::OpencodeServer(_)
            | EventKind::SettingsChanged { .. }
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
            | EventKind::Unknown => {}
        }
    }

    fn agent_status_heard(&mut self, report: &StatusReport) {
        let host = self.env_host(&report.agent.environment);
        self.statusline_heard(host, &report.status);
    }
}
