use gpui_kit::Context;
use krad_agents::{ErrorCode, Response, TerminalHandle};
use krad_domain::{AgentInfo, TerminalState};
use krad_domain::{AgentState, EnvironmentName, TerminalId};
use krad_runtime::{closed_worker_infos, BusReply, Mail, Pending};

use super::super::live_terminal::LiveTerminal;
use super::super::surface::ChatPresence;
use super::super::EnvironmentShell;
use super::feed::lists_as_agent;
use super::{Settlement, NO_SUCH_TERMINAL};

pub(super) const CLOSED_TERMINAL: &str = "That terminal has no running session";

pub(super) enum ScreenTail {
    Now(Response),
    Later(Pending<Option<Vec<String>>>),
}

impl From<Response> for ScreenTail {
    fn from(response: Response) -> Self {
        Self::Now(response)
    }
}

fn tail_response(tail: Option<Vec<String>>) -> Response {
    tail.map_or_else(
        || Response::error(ErrorCode::NotFound, CLOSED_TERMINAL),
        |lines| Response::Screen(lines.join("\n")),
    )
}

impl EnvironmentShell {
    pub(super) fn agent_infos(&self, environment: &str, me: Option<u64>) -> Vec<AgentInfo> {
        let env = EnvironmentName::new(environment.to_owned());
        let Some(live) = self.envs.get(&env) else {
            return Vec::new();
        };
        live.data
            .nodes
            .iter()
            .filter(|node| lists_as_agent(node))
            .map(|node| AgentInfo {
                is_self: Some(node.id.as_u64()) == me,
                ..self.agent_info(&env, live, node)
            })
            .collect()
    }

    pub(super) fn all_agent_infos(&self, environment: &str, me: Option<u64>) -> Vec<AgentInfo> {
        let mut agents = self.agent_infos(environment, me);
        if let Some(live) = self.envs.get(&EnvironmentName::new(environment.to_owned())) {
            agents.extend(closed_worker_infos(&live.data));
        }
        agents
    }

    pub(super) fn post(&self, from: &TerminalHandle, to: u64, body: String, reply: BusReply) {
        let env = EnvironmentName::new(from.environment.clone());
        let nodes = self.envs.get(&env).map(|live| &live.data.nodes);
        let Some(nodes) = nodes.filter(|nodes| nodes.iter().any(|node| node.id.as_u64() == to))
        else {
            reply
                .send(Response::error(ErrorCode::NotFound, NO_SUCH_TERMINAL))
                .unwrap_or_default();
            return;
        };
        let from_title = nodes
            .iter()
            .find(|node| node.id.as_u64() == from.terminal)
            .map_or_default(|node| node.title.clone());
        let mail = Mail {
            from: from.clone(),
            from_title,
            to,
            body,
        };
        reply.post_mail(mail).unwrap_or_default();
    }

    pub(super) fn settled_state(&self, target: &TerminalHandle) -> Settlement {
        self.find_settlement(target)
            .or_else(|| self.closed_settlement(target))
            .unwrap_or(Settlement::Missing)
    }

    fn closed_settlement(&self, target: &TerminalHandle) -> Option<Settlement> {
        let env = EnvironmentName::new(target.environment.clone());
        let (_, worker) = self
            .envs
            .get(&env)?
            .data
            .worker(TerminalId(target.terminal))?;
        worker
            .session
            .is_none()
            .then_some(Settlement::Settled(TerminalState::Closed))
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
                .is_some_and(LiveTerminal::is_running),
        };
        Some(match (alive, node.state) {
            (false, _) => Settlement::Settled(TerminalState::Exited),
            (true, AgentState::Working) => Settlement::Working,
            (true, state) => Settlement::Settled(TerminalState::Running(state)),
        })
    }

    pub(super) fn screen_tail(&self, environment: &str, target: u64, lines: usize) -> ScreenTail {
        let env = EnvironmentName::new(environment.to_owned());
        let id = TerminalId(target);
        if let Some(tail) = self.chat_tail(&env, id, lines) {
            return Response::Screen(tail).into();
        }
        let runs = self
            .envs
            .get(&env)
            .is_some_and(|live| live.sessions.contains_key(&id));
        if !runs {
            return Response::error(ErrorCode::NotFound, CLOSED_TERMINAL).into();
        }
        ScreenTail::Later(
            self.terminals()
                .terminal_lines(super::super::live_terminal::terminal_ref(&env, id), lines),
        )
    }

    pub(super) fn reply_tail(tail: ScreenTail, reply: BusReply, cx: &mut Context<Self>) {
        match tail {
            ScreenTail::Now(response) => reply.send(response).unwrap_or_default(),
            ScreenTail::Later(lines) => cx
                .spawn(async move |_, _| {
                    let response = tail_response(lines.await.ok().flatten());
                    reply.send(response).unwrap_or_default();
                })
                .detach(),
        }
    }
}
