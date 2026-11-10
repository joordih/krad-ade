use futures::channel::oneshot::Sender as ReplySender;
use gpui_kit::Context;
use krad_agents::{Caller, ErrorCode, LeadHandle, Request, Response, TerminalState};
use krad_domain::{AgentState, EnvironmentName, SessionId, TerminalId, VariantRunId};

use super::super::EnvironmentShell;
use super::chat::ChatSpawn;
use super::close::{pick_refusal, session_close_refusal};
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
    pub(super) fn as_lead(&self, from: Caller, request: &Request) -> Caller {
        let Caller::Terminal(terminal) = &from else {
            return from;
        };
        let Some(live) = self
            .envs
            .get(&EnvironmentName::new(terminal.environment.clone()))
        else {
            return from;
        };
        let Some(session) = live.data.session_led_by(TerminalId(terminal.terminal)) else {
            return from;
        };
        let follows_own_worker = request
            .followed_terminal()
            .and_then(|target| live.data.worker(TerminalId(target)))
            .is_some_and(|(owner, _)| owner == session);
        if !request.directs_workers() && !follows_own_worker {
            return from;
        }
        Caller::Lead(LeadHandle {
            environment: terminal.environment.clone(),
            session: session.as_u64(),
        })
    }

    pub(super) fn answer_lead(
        &mut self,
        lead: &LeadHandle,
        request: Request,
        reply: ReplySender<Response>,
        cx: &mut Context<Self>,
    ) {
        let session = SessionId(lead.session);
        let env = EnvironmentName::new(lead.environment.clone());
        let led_by = self
            .envs
            .get(&env)
            .and_then(|live| live.data.session(session))
            .and_then(|session| Some((session.group, session.lead.as_ref()?.terminal)));
        let group = led_by.map(|(group, _)| group);
        let mode = led_by
            .and_then(|(_, terminal)| terminal)
            .and_then(|terminal| self.chat_permission_mode(&env, terminal));
        let refusal = match group {
            None => NOT_A_LEAD_ANYMORE,
            Some(None) => NO_LEAD_GROUP,
            Some(Some(group)) => {
                match request {
                    Request::Close(order) => {
                        self.close_for_lead(&env, session, order, reply, cx);
                    }
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
                            mode,
                        };
                        self.start_chat_worker(&lead.environment, group, spawn, reply, cx);
                    }
                    request @ Request::CloseSession { session: closing } => {
                        let refused = self.envs.get(&env).and_then(|live| {
                            session_close_refusal(&live.data, session, SessionId(closing))
                        });
                        match refused {
                            Some(error) => reply.send(Response::Error(error)).unwrap_or_default(),
                            None => self.answer_chat(&lead.environment, group, request, reply, cx),
                        }
                    }
                    request @ Request::PickVariant { run, .. } => {
                        let run_group = self.envs.get(&env).and_then(|live| {
                            live.data
                                .variant_runs
                                .run(VariantRunId(run))
                                .map(|run| run.group)
                        });
                        match pick_refusal(run_group, group) {
                            Some(error) => reply.send(Response::Error(error)).unwrap_or_default(),
                            None => self.answer_chat(&lead.environment, group, request, reply, cx),
                        }
                    }
                    other @ (Request::MemorySearch { .. } | Request::MemoryRead { .. }) => {
                        let chat = led_by.and_then(|(_, terminal)| terminal);
                        let answer = self.answer_memory(&lead.environment, chat, &other);
                        reply.send(answer).unwrap_or_default();
                    }
                    other => {
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
            .send(Response::error(ErrorCode::Forbidden, refusal))
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
                    WorkerPlace::Closed => {
                        Some(Response::error(ErrorCode::NotFound, WORKER_RETIRED))
                    }
                    WorkerPlace::Open if self.has_finished(env, terminal) => {
                        let tail = self.screen_tail(env.as_str(), *target, *lines);
                        self.close_finished_worker(env, lead, terminal, cx);
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
                    state: TerminalState::Closed,
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
