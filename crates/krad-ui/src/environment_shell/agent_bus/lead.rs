use gpui_kit::Context;
use krad_domain::agents_api::{ErrorCode, LeadHandle, Request, Response};
use krad_domain::{EnvironmentName, SessionId, TerminalId, VariantRunId};
use krad_runtime::BusReply;

use super::super::EnvironmentShell;
use super::close::{pick_refusal, session_close_refusal};
use super::requests::CLOSED_TERMINAL;

const NOT_A_LEAD_ANYMORE: &str = "This chat is no longer its session's lead";
const NO_LEAD_GROUP: &str = "The lead's session belongs to no group, so it cannot start workers";

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
        reply: BusReply,
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
        let refusal = match group {
            None => NOT_A_LEAD_ANYMORE,
            Some(None) => NO_LEAD_GROUP,
            Some(Some(group)) => {
                match request {
                    Request::Close(_) | Request::Spawn { .. } => {
                        reply.send(super::answered_by_runtime()).unwrap_or_default();
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
                    Request::Send { to, body } => {
                        let response =
                            self.reopen_and_send(&env, session, TerminalId(to), &body, cx);
                        reply.send(response).unwrap_or_default();
                    }
                    other => self.answer_chat(&lead.environment, group, other, reply, cx),
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

    fn reopen_and_send(
        &mut self,
        env: &EnvironmentName,
        lead: SessionId,
        terminal: TerminalId,
        body: &str,
        cx: &mut Context<Self>,
    ) -> Response {
        let reopened = self.worker_place(env, lead, terminal) == Some(WorkerPlace::Closed)
            && self.reopen_worker(env, terminal, cx);
        if reopened && self.queue_chat_prompt(env, terminal, body, cx) {
            return Response::Submitted {
                terminal: terminal.as_u64(),
            };
        }
        Response::error(ErrorCode::NotFound, CLOSED_TERMINAL)
    }
}
