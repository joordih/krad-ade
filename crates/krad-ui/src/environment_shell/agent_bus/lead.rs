use futures::channel::oneshot::Sender as ReplySender;
use gpui_kit::Context;
use krad_agents::{LeadHandle, Request, Response, TerminalState};
use krad_domain::{AgentState, EnvironmentName, SessionId, TerminalId};

use super::super::EnvironmentShell;
use super::chat::ChatSpawn;
use super::{handle, Settlement};

const NOT_A_LEAD_ANYMORE: &str = "This chat is no longer its session's lead";
const NO_LEAD_GROUP: &str = "The lead's session belongs to no group, so it cannot start workers";
const WORKER_RETIRED: &str = "That worker finished and was closed; send it a message to reopen it";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorkerPlace {
    Open,
    Closed,
}

impl EnvironmentShell {
    pub(super) fn answer_lead(
        &mut self,
        lead: &LeadHandle,
        request: Request,
        reply: ReplySender<Response>,
        cx: &mut Context<Self>,
    ) {
        let session = SessionId(lead.session);
        let group = self
            .envs
            .get(&EnvironmentName::new(lead.environment.clone()))
            .and_then(|live| live.data.session(session))
            .filter(|session| session.lead.is_some())
            .map(|session| session.group);
        let refusal = match group {
            None => NOT_A_LEAD_ANYMORE,
            Some(None) => NO_LEAD_GROUP,
            Some(Some(group)) => {
                match request {
                    Request::Spawn {
                        agent,
                        worktree,
                        task,
                    } => {
                        let spawn = ChatSpawn {
                            agent,
                            worktree,
                            task,
                            lead: Some(session),
                        };
                        self.start_chat_worker(&lead.environment, group, spawn, reply, cx);
                    }
                    other => {
                        let env = EnvironmentName::new(lead.environment.clone());
                        if let Some(answer) = self.answer_worker(&env, session, &other, cx) {
                            reply.send(answer).unwrap_or_default();
                            return;
                        }
                        self.answer_chat(&lead.environment, group, other, reply, cx);
                    }
                }
                return;
            }
        };
        reply
            .send(Response::Error(refusal.to_owned()))
            .unwrap_or_default();
    }

    fn worker_place(
        &self,
        env: &EnvironmentName,
        lead: SessionId,
        terminal: TerminalId,
    ) -> Option<WorkerPlace> {
        let (owner, worker) = self.envs.get(env)?.data.worker(terminal)?;
        (owner == lead).then_some(if worker.session.is_some() {
            WorkerPlace::Open
        } else {
            WorkerPlace::Closed
        })
    }

    fn has_finished(&self, env: &EnvironmentName, terminal: TerminalId) -> bool {
        let settled = self.settled_state(&handle(env, terminal));
        let idle = matches!(
            settled,
            Settlement::Settled(TerminalState::Running(AgentState::Idle | AgentState::Done))
        );
        idle && !self.is_launching(env, terminal)
    }

    fn answer_worker(
        &mut self,
        env: &EnvironmentName,
        lead: SessionId,
        request: &Request,
        cx: &mut Context<Self>,
    ) -> Option<Response> {
        match request {
            Request::Read { target, lines } => {
                let terminal = TerminalId(*target);
                match self.worker_place(env, lead, terminal)? {
                    WorkerPlace::Closed => Some(Response::Error(WORKER_RETIRED.to_owned())),
                    WorkerPlace::Open if self.has_finished(env, terminal) => {
                        let tail = self.screen_tail(env.as_str(), *target, *lines);
                        self.close_terminal_in(env, terminal, cx);
                        Some(tail)
                    }
                    WorkerPlace::Open => None,
                }
            }
            Request::Wait { target, .. } => {
                let closed =
                    self.worker_place(env, lead, TerminalId(*target)) == Some(WorkerPlace::Closed);
                closed.then_some(Response::Settled {
                    target: *target,
                    state: TerminalState::Exited,
                })
            }
            Request::Send { to, .. } => {
                let terminal = TerminalId(*to);
                if self.worker_place(env, lead, terminal) == Some(WorkerPlace::Closed) {
                    self.reopen_worker(env, terminal, cx);
                }
                None
            }
            _ => None,
        }
    }
}
