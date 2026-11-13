use futures::channel::oneshot;
use krad_agents::{Caller, ErrorCode, Incoming, Request, Response, TerminalState};
use krad_domain::{AgentState, EnvironmentName, SessionId, TerminalId};
use krad_protocol::AgentRef;
use std::time::Instant;

use super::Runtime;

const CLOSED_TERMINAL: &str = "That terminal has no running session";

fn reply_with(reply: oneshot::Sender<Response>, response: Response) {
    if reply.send(response).is_err() {
        tracing::trace!("a bus caller left before its answer");
    }
}

impl Runtime {
    pub(super) fn answer_on_terminals(&mut self, incoming: Incoming) -> Option<Incoming> {
        let (environment, lead) = match &incoming.from {
            Caller::Terminal(terminal) => (terminal.environment.clone(), None),
            Caller::Lead(lead) => (lead.environment.clone(), Some(SessionId(lead.session))),
            Caller::Git(_) | Caller::Client => return Some(incoming),
        };
        let environment = EnvironmentName::new(environment);
        let target = match &incoming.request {
            Request::Wait { target, .. } | Request::Read { target, .. } => TerminalId(*target),
            Request::Send { to, .. } => TerminalId(*to),
            _ => return Some(incoming),
        };
        let target = AgentRef {
            environment,
            terminal: target,
        };
        if !self.terminals.knows(&target) || self.chats.is_open(&target) {
            return Some(incoming);
        }
        match (&incoming.request, lead) {
            (Request::Wait { .. }, _) => self.wait_on_terminal(target, incoming.reply),
            (Request::Read { lines, .. }, _) => {
                let lines = *lines;
                self.read_terminal(&target, lead, lines, incoming.reply);
            }
            (Request::Send { .. }, Some(lead)) => {
                return self.send_to_terminal(&target, lead, incoming)
            }
            _ => return Some(incoming),
        }
        None
    }

    pub(super) fn terminal_settled(&self, target: &AgentRef) -> Option<TerminalState> {
        let typed = self.terminals.typed(target)?;
        if !typed.running {
            return Some(TerminalState::Exited);
        }
        if self.typist.is_starting(target) {
            return None;
        }
        let state = self
            .environments
            .data(&target.environment)
            .and_then(|data| data.nodes.iter().find(|node| node.id == target.terminal))
            .map_or(AgentState::Idle, |node| node.state);
        (state != AgentState::Working).then_some(TerminalState::Running(state))
    }

    fn wait_on_terminal(&mut self, target: AgentRef, reply: oneshot::Sender<Response>) {
        match self.terminal_settled(&target) {
            Some(state) => reply_with(
                reply,
                Response::Settled {
                    target: target.terminal.as_u64(),
                    state,
                },
            ),
            None => self.workers.wait(target, reply),
        }
    }

    fn read_terminal(
        &mut self,
        target: &AgentRef,
        lead: Option<SessionId>,
        lines: usize,
        reply: oneshot::Sender<Response>,
    ) {
        let response = self.terminals.recent_lines(target, lines).map_or_else(
            || Response::error(ErrorCode::NotFound, CLOSED_TERMINAL),
            |lines| Response::Screen(lines.join("\n")),
        );
        reply_with(reply, response);
        let Some(lead) = lead else {
            return;
        };
        let owned_open = self
            .environments
            .data(&target.environment)
            .and_then(|data| data.worker(target.terminal))
            .is_some_and(|(owner, worker)| owner == lead && worker.session.is_some());
        let finished = matches!(
            self.terminal_settled(target),
            Some(TerminalState::Running(AgentState::Idle | AgentState::Done))
        );
        if owned_open && finished && !self.workers.is_launching(target) {
            self.close_finished_worker(&target.environment, lead, target.terminal);
        }
    }

    fn send_to_terminal(
        &mut self,
        target: &AgentRef,
        lead: SessionId,
        incoming: Incoming,
    ) -> Option<Incoming> {
        let closed_worker = self
            .environments
            .data(&target.environment)
            .and_then(|data| data.worker(target.terminal))
            .is_some_and(|(owner, worker)| owner == lead && worker.session.is_none());
        if closed_worker {
            return Some(incoming);
        }
        let running = self
            .terminals
            .typed(target)
            .is_some_and(|typed| typed.running);
        let Request::Send { body, .. } = &incoming.request else {
            return Some(incoming);
        };
        let response = if running {
            self.submit(target.clone(), body, Instant::now());
            Response::Submitted {
                terminal: target.terminal.as_u64(),
            }
        } else {
            Response::error(ErrorCode::NotFound, CLOSED_TERMINAL)
        };
        reply_with(incoming.reply, response);
        None
    }
}
