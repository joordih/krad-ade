use krad_agents::{Caller, ErrorCode, Incoming, Request, Response, TerminalState};
use krad_domain::{EnvironmentName, NodeKind, TerminalId};
use krad_protocol::AgentRef;

use super::working::reply_with;
use super::Runtime;

const NO_SUCH_TERMINAL: &str = "No terminal with that id in your environment";

enum Waited {
    Missing,
    Pending,
    Settled(TerminalState),
}

impl Runtime {
    pub(super) fn answer_waits(&mut self, incoming: Incoming) -> Option<Incoming> {
        let environment = match &incoming.from {
            Caller::Terminal(terminal) => terminal.environment.clone(),
            Caller::Lead(lead) => lead.environment.clone(),
            Caller::Git(_) | Caller::Client => return Some(incoming),
        };
        let Request::Wait { target, .. } = &incoming.request else {
            return Some(incoming);
        };
        let target = AgentRef {
            environment: EnvironmentName::new(environment),
            terminal: TerminalId(*target),
        };
        match self.wait_answer(&target) {
            Some(response) => reply_with(incoming.reply, response),
            None => self.workers.wait(target, incoming.reply),
        }
        None
    }

    pub(super) fn wait_answer(&self, target: &AgentRef) -> Option<Response> {
        let state = match self.waited(target) {
            Waited::Missing => {
                return Some(Response::error(ErrorCode::NotFound, NO_SUCH_TERMINAL));
            }
            Waited::Pending => return None,
            Waited::Settled(state) => state,
        };
        Some(Response::Settled {
            target: target.terminal.as_u64(),
            state,
        })
    }

    fn waited(&self, target: &AgentRef) -> Waited {
        if self.chats.is_open(target) || self.terminals.knows(target) {
            return self
                .settled_state(target)
                .map_or(Waited::Pending, Waited::Settled);
        }
        let Some(data) = self.environments.data(&target.environment) else {
            return Waited::Missing;
        };
        let closed_worker = data
            .worker(target.terminal)
            .is_some_and(|(_, worker)| worker.session.is_none());
        if closed_worker {
            return Waited::Settled(TerminalState::Closed);
        }
        if self.workers.is_launching(target) {
            return Waited::Pending;
        }
        match data.nodes.iter().find(|node| node.id == target.terminal) {
            None => Waited::Missing,
            Some(node) if node.kind == NodeKind::Chat => Waited::Pending,
            Some(_) => Waited::Settled(TerminalState::Exited),
        }
    }
}
