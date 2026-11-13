mod access;
mod asks;
mod delegation;
mod directory;
mod discovery;
mod lead;
mod mail;

pub(crate) use access::open;
pub use access::{BusAccess, BusSetup};
pub use delegation::{BusReply, Delegated, DelegationReceiver, Mail};
pub(crate) use delegation::{DelegationId, DELEGATION_BUFFER};
pub use lead::closed_worker_infos;
pub(crate) use lead::{as_lead, closed_worker_answer};

use futures::channel::{mpsc, oneshot};
use krad_agents::{
    lead_refusal, AgentInfo, AgentKey, AgentStatus, Caller, Incoming, Request, Response,
    StateReason, TerminalHandle,
};
use krad_domain::{EnvironmentName, TerminalId};
use krad_protocol::{
    AgentRef, AskRequest, BusCommand, BusSnapshot, EventKind, Reply, StatusReport,
};
use std::collections::HashMap;
use std::sync::mpsc::SyncSender;
use std::sync::Arc;

use crate::input::Input;
use asks::Asks;
use delegation::Delegations;
use directory::Directory;
use mail::Mailboxes;

fn reply_with(reply: oneshot::Sender<Response>, response: Response) {
    if reply.send(response).is_err() {
        tracing::trace!("a bus caller left before its answer");
    }
}

fn agent_of(handle: &TerminalHandle) -> AgentRef {
    AgentRef {
        environment: EnvironmentName::new(handle.environment.clone()),
        terminal: TerminalId(handle.terminal),
    }
}

#[derive(Debug)]
pub(crate) struct Bus {
    access: Arc<BusAccess>,
    directory: Directory,
    mail: Mailboxes,
    statuses: HashMap<AgentRef, AgentStatus>,
    asks: Asks,
    delegations: Delegations,
}

impl Bus {
    pub(crate) fn new(access: Arc<BusAccess>, inbox: SyncSender<Input>) -> Self {
        let directory = Directory::new(access.endpoint.feed.clone());
        Self {
            access,
            directory,
            mail: Mailboxes::default(),
            statuses: HashMap::new(),
            asks: Asks::default(),
            delegations: Delegations::new(inbox),
        }
    }

    pub(crate) fn access(&self) -> &BusAccess {
        &self.access
    }

    #[tracing::instrument(skip_all)]
    pub(crate) fn answer(&mut self, incoming: Incoming, outbox: &mut Vec<EventKind>) {
        let Incoming {
            from,
            request,
            reply,
        } = incoming;
        let Caller::Terminal(terminal) = &from else {
            if matches!((&from, &request), (Caller::Client, Request::List)) {
                reply_with(reply, Response::Agents(self.directory.every_agent()));
                return;
            }
            self.delegate(from, request, reply);
            return;
        };
        match request {
            Request::Check { wait_secs } => {
                self.mail.check(terminal.clone(), wait_secs, reply, outbox);
            }
            Request::Status(status) => {
                let agent = agent_of(terminal);
                outbox.push(EventKind::AgentStatus(StatusReport {
                    agent: agent.clone(),
                    status: status.clone(),
                }));
                self.statuses.insert(agent, status);
                reply_with(reply, Response::Noted);
            }
            Request::Ask {
                tool,
                input,
                suggestions,
                call,
            } => {
                let request = AskRequest {
                    tool,
                    input,
                    suggestions,
                    call,
                };
                self.asks.hold(agent_of(terminal), request, reply, outbox);
            }
            Request::List => {
                let agents = self
                    .directory
                    .agents_in(&terminal.environment, Some(terminal.terminal));
                reply_with(reply, Response::Agents(agents));
            }
            request => self.delegate(from, request, reply),
        }
    }

    fn delegate(&mut self, from: Caller, request: Request, reply: oneshot::Sender<Response>) {
        if let Some(refusal) = lead_refusal(&from, &request) {
            reply_with(reply, refusal);
            return;
        }
        self.delegations.delegate(from, request, reply);
    }

    pub(crate) fn execute(
        &mut self,
        command: &BusCommand,
        outbox: &mut Vec<EventKind>,
    ) -> Option<Reply> {
        match command {
            BusCommand::AnswerAsk { ask, decision } => {
                Some(self.asks.answer(*ask, decision, outbox))
            }
            BusCommand::PublishDiscovery { file } => Some(discovery::publish(&self.access, *file)),
            BusCommand::ReplayHook { .. } | BusCommand::Unknown => None,
        }
    }

    pub(crate) fn respond(&mut self, delegation: DelegationId, response: Response) {
        self.delegations.respond(delegation, response);
    }

    pub(crate) fn post_mail(
        &mut self,
        delegation: DelegationId,
        mail: Mail,
        outbox: &mut Vec<EventKind>,
    ) {
        let id = self.mail.post(mail, outbox);
        self.delegations.respond(delegation, Response::Sent { id });
    }

    pub(crate) fn dropped(&mut self, delegation: DelegationId) {
        self.delegations.forget(delegation);
    }

    pub(crate) fn delegate_to(&mut self, delegate: mpsc::Sender<Delegated>) {
        self.delegations.delegate_to(delegate);
    }

    pub(crate) fn observe(
        &mut self,
        key: &AgentKey,
        seen: Option<AgentInfo>,
        hint: Option<StateReason>,
        outbox: &mut Vec<EventKind>,
    ) {
        if self.directory.observe(key, seen, hint) {
            self.mail.forget(key, outbox);
        }
    }

    pub(crate) fn reconcile(
        &mut self,
        environments: &[String],
        agents: Vec<AgentInfo>,
        outbox: &mut Vec<EventKind>,
    ) {
        for gone in self.directory.reconcile(environments, agents) {
            self.mail.forget(&gone, outbox);
        }
    }

    pub(crate) fn agents_in(&self, environment: &str, me: Option<u64>) -> Vec<AgentInfo> {
        self.directory.agents_in(environment, me)
    }

    pub(crate) fn every_agent(&self) -> Vec<AgentInfo> {
        self.directory.every_agent()
    }

    pub(crate) fn tick(&mut self, outbox: &mut Vec<EventKind>) {
        self.mail.tick();
        self.asks.tick(outbox);
        self.delegations.tick();
    }

    pub(crate) fn snapshot(&self) -> BusSnapshot {
        BusSnapshot {
            statuses: self
                .statuses
                .iter()
                .map(|(agent, status)| StatusReport {
                    agent: agent.clone(),
                    status: status.clone(),
                })
                .collect(),
            mail: self.mail.counts(),
            asks: self.asks.pending(),
        }
    }
}
