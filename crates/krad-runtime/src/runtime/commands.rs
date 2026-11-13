use krad_agents::{Caller, Incoming};
use krad_domain::EnvironmentName;
use krad_protocol::{Command, ErrorCode, EventKind, Reply, RequestId, RuntimeCommand};

use super::{refused, Runtime, NO_BUS, UNSUPPORTED_COMMAND};
use crate::bus::{as_lead, closed_worker_answer};
use crate::input::Input;

impl Runtime {
    pub(super) fn execute(&mut self, id: RequestId, command: Command) -> Reply {
        match command {
            Command::Runtime(RuntimeCommand::Ping) => {
                self.outbox.push(EventKind::Pong { request: id });
                Reply::Accepted
            }
            Command::Bus(command) => {
                let Some(bus) = &mut self.bus else {
                    return refused(ErrorCode::Unsupported, NO_BUS);
                };
                bus.execute(&command, &mut self.outbox)
                    .unwrap_or_else(|| refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND))
            }
            Command::Events(command) => self
                .events
                .execute(&command, &mut self.outbox)
                .unwrap_or_else(|| refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND)),
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
                .execute_environments(command)
                .unwrap_or_else(|| refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND)),
            Command::Host(command) => self
                .execute_host(command)
                .unwrap_or_else(|| refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND)),
            Command::Chat(command) => self
                .execute_chat(command)
                .unwrap_or_else(|| refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND)),
            Command::Gateway(command) => self
                .gateway
                .execute(command, &self.jobs, &mut self.outbox)
                .unwrap_or_else(|| refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND)),
            Command::Settings(command) => {
                let reply = self.settings.execute(command, &mut self.outbox);
                self.flush_chats();
                reply.unwrap_or_else(|| refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND))
            }
            Command::Terminal(command) => self
                .execute_terminal(command)
                .unwrap_or_else(|| refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND)),
            Command::Runtime(RuntimeCommand::Unknown) | Command::Unknown => {
                refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND)
            }
        }
    }

    pub(super) fn answer_bus(&mut self, incoming: Incoming) {
        let Some(bus) = &mut self.bus else {
            tracing::debug!("a bus request reached a runtime without a bus");
            return;
        };
        let Incoming {
            from,
            request,
            reply,
        } = incoming;
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
        bus.answer(incoming, &mut self.outbox);
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
            | Input::OfferMemoryTools(_)
            | Input::OpenTerminal { .. }
            | Input::TerminalAction { .. }
            | Input::TerminalPalette(_)
            | Input::OpenSignIn { .. }
            | Input::MachineChannel { .. }
            | Input::OpenChat { .. }
            | Input::CloseChat(_)
            | Input::ChatPage { .. }
            | Input::MentionIndex { .. }
            | Input::ForgetMentionIndexes
            | Input::StartLiveChat(_)
            | Input::Opencode(_)
            | Input::Wake
            | Input::Tick
            | Input::Stop => {}
        }
    }
}
