use krad_agents::{Caller, Incoming, Request};
use krad_domain::{AppEvent, EnvironmentEdit, EnvironmentName, NodeEdit};
use krad_protocol::{
    AgentRef, BusCommand, Command, ErrorCode, EventKind, EventsCommand, HookOrigin, Reply,
    RequestId, RuntimeCommand,
};

use super::hooking::{agent_of, hook_report};
use super::{refused, Runtime, NO_BUS, UNSUPPORTED_COMMAND};
use crate::bus::{as_lead, closed_worker_answer};
use crate::environments::UNKNOWN_ENVIRONMENT;
use crate::git::answer_git;
use crate::input::Input;

impl Runtime {
    pub(super) fn execute(&mut self, id: RequestId, command: Command) -> Reply {
        match command {
            Command::Runtime(RuntimeCommand::Ping) => {
                self.outbox.push(EventKind::Pong { request: id });
                Reply::Accepted
            }
            Command::Bus(BusCommand::ReplayHook {
                agent,
                report,
                delivered,
            }) => {
                self.hear_hook(agent, report, HookOrigin::Replayed { delivered });
                Reply::Accepted
            }
            Command::Bus(command) => {
                let Some(bus) = &mut self.bus else {
                    return refused(ErrorCode::Unsupported, NO_BUS);
                };
                bus.execute(&command, &mut self.outbox)
                    .unwrap_or_else(|| refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND))
            }
            Command::Events(EventsCommand::WatchCodeHosts { watch }) => {
                self.code_hosts.asked = watch.is_on();
                Reply::Accepted
            }
            Command::Events(command) => {
                self.note_finished_agent(&command);
                self.webhooks_edited(&command);
                self.events
                    .execute(&command, &mut self.outbox)
                    .unwrap_or_else(|| refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND))
            }
            Command::Attention(command) => self
                .attention
                .execute(&command, &mut self.outbox)
                .unwrap_or_else(|| refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND)),
            Command::Usage(command) => self
                .usage
                .execute(&command, &self.jobs, &mut self.outbox)
                .unwrap_or_else(|| refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND)),
            Command::Models(command) => self
                .models
                .execute(&command, &mut self.outbox)
                .unwrap_or_else(|| refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND)),
            Command::Environments(command) => self
                .execute_environments(command, id)
                .unwrap_or_else(|| refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND)),
            Command::Session(command) => self.execute_edit(id, command.into()),
            Command::Group(command) => self.execute_edit(id, command.into()),
            Command::Node(command) => self.execute_edit(id, command.into()),
            Command::Host(command) => self
                .execute_host(command)
                .unwrap_or_else(|| refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND)),
            Command::Chat(command) => self
                .execute_chat(command)
                .unwrap_or_else(|| refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND)),
            Command::Gateway(command) => self
                .gateway
                .execute(command, (&self.jobs, &self.secrets), &mut self.outbox)
                .unwrap_or_else(|| refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND)),
            Command::Settings(command) => {
                let reply = self.settings.execute(command, &mut self.outbox);
                self.runners.webhooks.check_soon();
                self.flush_chats();
                self.run_ready_stops();
                reply.unwrap_or_else(|| refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND))
            }
            Command::Terminal(command) => self
                .execute_terminal(command)
                .unwrap_or_else(|| refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND)),
            Command::Git(command) => self.execute_git(id, command),
            Command::Changes(command) => self.execute_changes(id, command),
            Command::Kraddie(command) => self.execute_kraddie(command),
            Command::Variant(command) => self.execute_variant(command),
            Command::Review(command) => self.execute_review(id, command),
            Command::CodeHost(command) => self.execute_code_host(command),
            Command::Runner(command) => self.execute_runner(command),
            Command::Runtime(RuntimeCommand::Unknown) | Command::Unknown => {
                refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND)
            }
        }
    }

    fn execute_edit(
        &mut self,
        id: RequestId,
        (environment, edit): (EnvironmentName, EnvironmentEdit),
    ) -> Reply {
        if edit.is_unknown() {
            self.environments
                .edit(environment, &edit, Some(id), &mut self.outbox);
            return refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND);
        }
        if let EnvironmentEdit::Node(NodeEdit::Close { terminal }) = &edit {
            self.workers.closed(&AgentRef {
                environment: environment.clone(),
                terminal: *terminal,
            });
        }
        match self
            .environments
            .edit(environment, &edit, Some(id), &mut self.outbox)
        {
            Some(_) => Reply::Accepted,
            None => refused(ErrorCode::NotFound, UNKNOWN_ENVIRONMENT),
        }
    }

    fn note_finished_agent(&mut self, command: &EventsCommand) {
        if let EventsCommand::Publish {
            environment,
            group: Some(group),
            event: AppEvent::AgentFinished { .. },
            ..
        } = command
        {
            self.git.sweep.note_finished(environment, *group);
        }
    }

    pub(super) fn answer_bus(&mut self, incoming: Incoming) {
        if self.bus.is_none() {
            tracing::debug!("a bus request reached a runtime without a bus");
            return;
        }
        let Incoming {
            from,
            request,
            reply,
        } = incoming;
        if let Caller::Git(_) = &from {
            answer_git(request, reply, self.hosts.prompts());
            return;
        }
        if let (Caller::Terminal(terminal), Request::Hook { .. }) = (&from, &request) {
            let agent = agent_of(terminal);
            if let Some(report) = hook_report(request) {
                let answer = self.hear_hook(agent, report, HookOrigin::Live);
                if reply.send(answer).is_err() {
                    tracing::trace!("a hook left before its answer");
                }
            }
            return;
        }
        let data = match &from {
            Caller::Terminal(terminal) => self
                .environments
                .data(&EnvironmentName::new(terminal.environment.clone())),
            Caller::Lead(_) | Caller::Git(_) | Caller::Client => None,
        };
        let from = as_lead(from, &request, data);
        if let Some(answer) = closed_worker_answer(&from, &request, data) {
            if reply.send(answer).is_err() {
                tracing::trace!("a bus caller left before its answer");
            }
            return;
        }
        let incoming = Incoming {
            from,
            request,
            reply,
        };
        let Some(incoming) = self
            .list_all(incoming)
            .and_then(|incoming| self.answer_kraddie_requests(incoming))
            .and_then(|incoming| self.answer_git_requests(incoming))
            .and_then(|incoming| self.answer_variant_requests(incoming))
            .and_then(|incoming| self.answer_for_leads(incoming))
            .and_then(|incoming| self.answer_on_chats(incoming))
            .and_then(|incoming| self.answer_on_terminals(incoming))
        else {
            return;
        };
        if let Some(bus) = &mut self.bus {
            bus.answer(incoming, &mut self.outbox);
        }
    }

    pub(super) fn apply_bus(&mut self, input: Input) {
        let Some(bus) = &mut self.bus else {
            return;
        };
        let outbox = &mut self.outbox;
        match input {
            Input::BusRespond {
                delegation,
                response,
            } => bus.respond(delegation, response),
            Input::BusMail { delegation, mail } => bus.post_mail(delegation, mail, outbox),
            Input::BusDropped(delegation) => bus.dropped(delegation),
            Input::Delegate(delegate) => bus.delegate_to(delegate),
            Input::ObserveAgent { key, seen, hint } => bus.observe(&key, seen, hint, outbox),
            Input::ReconcileAgents {
                environments,
                agents,
            } => bus.reconcile(&environments, agents, outbox),
            Input::Command { .. }
            | Input::Subscribe(_)
            | Input::Snapshot { .. }
            | Input::Bus(_)
            | Input::WebhookTested(_)
            | Input::CheckHarnesses { .. }
            | Input::PlaceUsage(_)
            | Input::ListModels { .. }
            | Input::JobDone(_)
            | Input::Settle(_)
            | Input::SettingsNotSaved(_)
            | Input::KraddieNotSaved(_)
            | Input::DigestHeard { .. }
            | Input::OpenTerminal { .. }
            | Input::TerminalAction { .. }
            | Input::TerminalPalette(_)
            | Input::OpenSignIn { .. }
            | Input::MachineChannel { .. }
            | Input::OpenChat { .. }
            | Input::CloseChat(_)
            | Input::ChatPage { .. }
            | Input::MentionIndex { .. }
            | Input::OfferBuffers { .. }
            | Input::ForgetMentionIndexes
            | Input::StartLiveChat(_)
            | Input::Opencode(_)
            | Input::Wake
            | Input::Tick
            | Input::Stop => {}
        }
    }
}
