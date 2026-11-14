use krad_agents::{AgentKey, Caller, Incoming, Request};
use krad_domain::{AppEvent, Edited, EnvironmentEdit, EnvironmentName, NodeEdit};
use krad_protocol::{
    AgentRef, BusCommand, Command, ErrorCode, EventKind, EventsCommand, HookOrigin, HostCommand,
    Reply, RequestId, RuntimeCommand, UsageCommand,
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
            Command::Bus(command) => self.execute_bus(command),
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
            Command::Usage(UsageCommand::Place { spot }) => self.place_usage(spot.as_ref()),
            Command::Usage(command) => self
                .usage
                .execute(&command, &self.jobs, &mut self.outbox)
                .unwrap_or_else(|| refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND)),
            Command::Models(command) => self
                .list_models(&command)
                .or_else(|| self.models.execute(&command, &mut self.outbox))
                .unwrap_or_else(|| refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND)),
            Command::Health(command) => self
                .execute_health(command)
                .unwrap_or_else(|| refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND)),
            Command::Opencode(command) => self
                .execute_opencode(command)
                .unwrap_or_else(|| refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND)),
            Command::Environments(command) => self
                .execute_environments(command, id)
                .unwrap_or_else(|| refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND)),
            Command::Session(command) => self.execute_edit(id, command.into()),
            Command::Group(command) => self.execute_edit(id, command.into()),
            Command::Node(command) => self.execute_edit(id, command.into()),
            Command::Host(
                ask @ (HostCommand::SshHosts
                | HostCommand::AcpPackages { .. }
                | HostCommand::InstallAcpPackage { .. }),
            ) => self.execute_host_ask(id, ask),
            Command::Repo(command) => self.execute_repo(id, command),
            Command::File(command) => self.execute_file(id, command),
            Command::Host(command) => self
                .execute_host(command)
                .unwrap_or_else(|| refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND)),
            Command::Chat(command) => self
                .execute_chat(id, command)
                .unwrap_or_else(|| refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND)),
            Command::Gateway(command) => self
                .gateway
                .execute(command, (&self.jobs, &self.secrets), &mut self.outbox)
                .unwrap_or_else(|| refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND)),
            Command::Settings(command) => {
                let reply = self.settings.execute(command, &mut self.outbox);
                self.hosts.use_timeouts(self.settings.current.host_timeouts);
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
            Command::Agent(command) => self.execute_agent(command),
            Command::Kraddie(command) => self.execute_kraddie(command),
            Command::Variant(command) => self.execute_variant(command),
            Command::Review(command) => self.execute_review(id, command),
            Command::CodeHost(command) => self.execute_code_host(command),
            Command::Runner(command) => self.execute_runner(command),
            Command::Cloud(command) => self.execute_cloud(command),
            Command::Runtime(RuntimeCommand::Settle | RuntimeCommand::Unknown)
            | Command::Unknown => refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND),
        }
    }

    fn execute_bus(&mut self, command: BusCommand) -> Reply {
        if let BusCommand::ReplayHook {
            agent,
            report,
            delivered,
        } = command
        {
            self.hear_hook(agent, report, HookOrigin::Replayed { delivered });
            return Reply::Accepted;
        }
        let Some(bus) = &mut self.bus else {
            return refused(ErrorCode::Unsupported, NO_BUS);
        };
        let outbox = &mut self.outbox;
        match command {
            BusCommand::ObserveAgent { agent, seen, hint } => {
                let key = AgentKey::new(agent.environment.as_str(), agent.terminal.as_u64());
                bus.observe(&key, seen, hint, outbox);
            }
            BusCommand::ReconcileAgents {
                environments,
                agents,
            } => {
                let environments: Vec<String> = environments
                    .iter()
                    .map(|name| name.as_str().to_owned())
                    .collect();
                bus.reconcile(&environments, agents, outbox);
            }
            command => {
                return bus
                    .execute(&command, outbox)
                    .unwrap_or_else(|| refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND));
            }
        }
        Reply::Accepted
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
            let node = AgentRef {
                environment: environment.clone(),
                terminal: *terminal,
            };
            self.stop_cloud_node(&node);
            self.workers.closed(&node);
        }
        match self
            .environments
            .edit(environment.clone(), &edit, Some(id), &mut self.outbox)
        {
            Some(Edited::Deleted(chats)) => {
                self.forget_archived(environment, chats);
                Reply::Accepted
            }
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
            Input::Command { .. }
            | Input::Subscribe { .. }
            | Input::Snapshot { .. }
            | Input::Bus(_)
            | Input::WebhookTested(_)
            | Input::JobDone(_)
            | Input::SettingsNotSaved(_)
            | Input::KraddieNotSaved(_)
            | Input::DigestHeard { .. }
            | Input::OpenTerminal { .. }
            | Input::TerminalAction { .. }
            | Input::TerminalPalette(_)
            | Input::OpenSignIn { .. }
            | Input::Runner(_)
            | Input::MachineChannel { .. }
            | Input::OpenChat { .. }
            | Input::OfferBuffers { .. }
            | Input::Opencode(_)
            | Input::Wake
            | Input::Tick
            | Input::Stop => {}
        }
    }
}
