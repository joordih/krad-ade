use krad_agents::{AgentInfo, Response, TerminalHandle, TerminalState};
use krad_domain::{AgentState, EnvironmentName, TerminalId};

use super::super::surface::ChatPresence;
use super::super::EnvironmentShell;
use super::{handle, Settlement, NO_SUCH_TERMINAL};

pub(super) const CLOSED_TERMINAL: &str = "That terminal has no running session";

impl EnvironmentShell {
    pub(super) fn agent_infos(&self, environment: &str, me: Option<u64>) -> Vec<AgentInfo> {
        let env = EnvironmentName::new(environment.to_owned());
        let Some(live) = self.envs.get(&env) else {
            return Vec::new();
        };
        live.data
            .nodes
            .iter()
            .filter(|node| node.file.is_none())
            .map(|node| {
                let session = live.sessions.get(&node.id);
                let chat = self.chat_presence(&env, node.id).is_some();
                let alive = chat || session.is_some_and(|session| session.state.is_running());
                AgentInfo {
                    terminal: node.id.as_u64(),
                    title: node.title.clone(),
                    group: live.data.group_of(node.id).map(|group| group.name.clone()),
                    state: if alive {
                        TerminalState::Running(node.state)
                    } else {
                        TerminalState::Exited
                    },
                    harness: session
                        .and_then(|session| session.harness)
                        .or(node.harness.filter(|_| chat)),
                    is_self: Some(node.id.as_u64()) == me,
                }
            })
            .collect()
    }

    pub(super) fn post(&mut self, from: &TerminalHandle, to: u64, body: String) -> Response {
        let env = EnvironmentName::new(from.environment.clone());
        let Some(live) = self.envs.get_mut(&env) else {
            return Response::Error(NO_SUCH_TERMINAL.to_owned());
        };
        if !live.data.nodes.iter().any(|node| node.id.as_u64() == to) {
            return Response::Error(NO_SUCH_TERMINAL.to_owned());
        }
        let from_title = live
            .data
            .nodes
            .iter()
            .find(|node| node.id.as_u64() == from.terminal)
            .map_or_default(|node| node.title.clone());
        let id = live.mailbox.post(from.terminal, from_title, to, body);
        self.wake_inbox_waiter(&handle(&env, TerminalId(to)));
        Response::Sent { id }
    }

    pub(super) fn take_mail(&mut self, terminal: &TerminalHandle) -> Vec<krad_agents::Message> {
        self.envs
            .get_mut(&EnvironmentName::new(terminal.environment.clone()))
            .map_or_default(|live| live.mailbox.take(terminal.terminal))
    }

    fn wake_inbox_waiter(&mut self, recipient: &TerminalHandle) {
        let Some(bus) = &mut self.agent_bus else {
            return;
        };
        bus.inbox_waiters.retain(|(_, reply)| !reply.is_canceled());
        let Some(index) = bus
            .inbox_waiters
            .iter()
            .position(|(waiter, _)| waiter == recipient)
        else {
            return;
        };
        let (_, reply) = bus.inbox_waiters.swap_remove(index);
        let messages = self.take_mail(recipient);
        reply.send(Response::Messages(messages)).unwrap_or_default();
    }

    pub(super) fn settled_state(&self, target: &TerminalHandle) -> Settlement {
        self.find_settlement(target).unwrap_or(Settlement::Missing)
    }

    fn find_settlement(&self, target: &TerminalHandle) -> Option<Settlement> {
        let env = EnvironmentName::new(target.environment.clone());
        let live = self.envs.get(&env)?;
        let node = live
            .data
            .nodes
            .iter()
            .find(|node| node.id.as_u64() == target.terminal)?;
        let alive = match self.chat_presence(&env, node.id) {
            Some(ChatPresence::Queued) => return Some(Settlement::Working),
            Some(ChatPresence::Open) => true,
            None => live
                .sessions
                .get(&node.id)
                .is_some_and(|session| session.state.is_running()),
        };
        Some(match (alive, node.state) {
            (false, _) => Settlement::Settled(TerminalState::Exited),
            (true, AgentState::Working) => Settlement::Working,
            (true, state) => Settlement::Settled(TerminalState::Running(state)),
        })
    }

    pub(super) fn screen_tail(&self, environment: &str, target: u64, lines: usize) -> Response {
        let env = EnvironmentName::new(environment.to_owned());
        let id = TerminalId(target);
        if let Some(tail) = self.chat_tail(&env, id, lines) {
            return Response::Screen(tail);
        }
        let session = self.envs.get(&env).and_then(|live| live.sessions.get(&id));
        match session {
            Some(session) => Response::Screen(session.engine.recent_lines(lines).join("\n")),
            None => Response::Error(CLOSED_TERMINAL.to_owned()),
        }
    }
}
