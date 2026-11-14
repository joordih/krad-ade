use futures::channel::oneshot;
use krad_agents::{
    Caller, CloseOrder, CloseTarget, Closed, ErrorCode, Incoming, LeadHandle, Request, Response,
    WorktreeFate, WorktreeLeft, WorktreeOutcome,
};
use krad_chat::{new_session_id, LiveSession};
use krad_domain::{
    worker_mode, AgentPreset, AgentState, ClosePlan, CloseRefusal, CloseSubject, EnvironmentName,
    GroupId, NodeEdit, NodePlace, Prompt, SessionEdit, SessionId, TerminalId, WhenWorking,
};
use krad_protocol::AgentRef;
use std::time::SystemTime;

use super::Runtime;
use crate::chats::{ChatBrief, ChatRole, ChatStart};
use crate::environments::UNKNOWN_ENVIRONMENT;
use crate::workers::{
    closed_target, plan_worker, refusal_error, worker_node, PendingWorker, SpawnPlan, WorkerAsker,
    WorkerOrder,
};

pub(super) enum LeadClose {
    Removing(Vec<ClosePlan>),
    Refused(CloseRefusal),
    Closed(Vec<Closed>),
}

pub(super) struct SpawnRefused {
    pub(super) reply: oneshot::Sender<Response>,
    pub(super) response: Response,
}

pub(super) fn reply_with(reply: oneshot::Sender<Response>, response: Response) {
    if reply.send(response).is_err() {
        tracing::trace!("a lead left before its answer");
    }
}

pub(super) fn agent(environment: &EnvironmentName, terminal: TerminalId) -> AgentRef {
    AgentRef {
        environment: environment.clone(),
        terminal,
    }
}

impl Runtime {
    pub(super) fn answer_for_leads(&mut self, incoming: Incoming) -> Option<Incoming> {
        if let (
            Caller::Terminal(caller),
            Request::Spawn {
                agent,
                worktree,
                task,
            },
        ) = (&incoming.from, &incoming.request)
        {
            let caller = AgentRef {
                environment: EnvironmentName::new(caller.environment.clone()),
                terminal: TerminalId(caller.terminal),
            };
            let order = (*agent, worktree.as_deref(), task.as_str());
            self.spawn_for_caller(&caller, order, incoming.reply);
            return None;
        }
        let Caller::Lead(lead) = &incoming.from else {
            return Some(incoming);
        };
        let lead = lead.clone();
        match &incoming.request {
            Request::Spawn {
                agent,
                worktree,
                task,
            } => {
                let (agent, worktree, task) = (*agent, worktree.clone(), task.clone());
                self.spawn_worker(&lead, (agent, worktree.as_deref(), &task), incoming)
            }
            Request::Close(order) => {
                let order = *order;
                match self.close_as_lead(&lead, order) {
                    LeadClose::Removing(plans) => {
                        let environment = EnvironmentName::new(lead.environment.clone());
                        self.close_removing(&environment, &plans, incoming.reply);
                        None
                    }
                    LeadClose::Refused(refusal) => {
                        reply_with(incoming.reply, Response::Error(refusal_error(refusal)));
                        None
                    }
                    LeadClose::Closed(closed) => {
                        reply_with(incoming.reply, Response::Closed(closed));
                        None
                    }
                }
            }
            Request::CloseSession { session } => {
                let session = *session;
                let order = CloseOrder {
                    target: CloseTarget::Session(session),
                    when_working: WhenWorking::CancelTurn,
                    worktree: WorktreeFate::Keep,
                };
                match self.close_as_lead(&lead, order) {
                    LeadClose::Removing(_) => Some(incoming),
                    LeadClose::Refused(refusal) => {
                        reply_with(incoming.reply, Response::Error(refusal_error(refusal)));
                        None
                    }
                    LeadClose::Closed(_) => {
                        reply_with(incoming.reply, Response::SessionClosed { session });
                        None
                    }
                }
            }
            _ => Some(incoming),
        }
    }

    fn spawn_worker(
        &mut self,
        lead: &LeadHandle,
        worker: (AgentPreset, Option<&str>, &str),
        incoming: Incoming,
    ) -> Option<Incoming> {
        if let Err(refused) = self.start_worker(lead, worker, incoming.reply) {
            reply_with(refused.reply, refused.response);
        }
        None
    }

    pub(super) fn start_worker(
        &mut self,
        lead: &LeadHandle,
        (preset, worktree, task): (AgentPreset, Option<&str>, &str),
        reply: oneshot::Sender<Response>,
    ) -> Result<(), SpawnRefused> {
        let environment = EnvironmentName::new(lead.environment.clone());
        let order = WorkerOrder {
            asker: WorkerAsker::Lead(SessionId(lead.session)),
            preset,
            worktree,
            task,
        };
        self.spawn_ordered(&environment, &order, reply)
    }

    pub(super) fn spawn_for_caller(
        &mut self,
        caller: &AgentRef,
        (preset, worktree, task): (AgentPreset, Option<&str>, &str),
        reply: oneshot::Sender<Response>,
    ) {
        let order = WorkerOrder {
            asker: WorkerAsker::Caller(caller.terminal),
            preset,
            worktree,
            task,
        };
        if let Err(refused) = self.spawn_ordered(&caller.environment, &order, reply) {
            reply_with(refused.reply, refused.response);
        }
    }

    fn spawn_ordered(
        &mut self,
        environment: &EnvironmentName,
        order: &WorkerOrder<'_>,
        reply: oneshot::Sender<Response>,
    ) -> Result<(), SpawnRefused> {
        let Some(data) = self.environments.data(environment) else {
            let response = Response::error(ErrorCode::NotFound, UNKNOWN_ENVIRONMENT);
            return Err(SpawnRefused { reply, response });
        };
        let plan = match plan_worker(data, &self.settings.current.acp, order) {
            Ok(plan) => plan,
            Err(response) => return Err(SpawnRefused { reply, response }),
        };
        let SpawnPlan {
            group,
            spawn,
            brief,
            mode_of,
            cloud,
        } = plan;
        let mode =
            mode_of.and_then(|terminal| self.chats.permission_mode(&agent(environment, terminal)));
        let pending = PendingWorker {
            spawn,
            preset: order.preset,
            brief,
            mode,
            cloud,
            reply,
        };
        match order.worktree {
            Some(name) => self.spawn_in_worktree(environment, group, name, pending),
            None => self.open_worker(environment, pending, group),
        }
        Ok(())
    }

    pub(super) fn open_worker(
        &mut self,
        environment: &EnvironmentName,
        pending: PendingWorker,
        group: GroupId,
    ) {
        let Some(data) = self.environments.data(environment) else {
            reply_with(
                pending.reply,
                Response::error(ErrorCode::NotFound, UNKNOWN_ENVIRONMENT),
            );
            return;
        };
        let PendingWorker {
            spawn,
            preset,
            brief,
            mode,
            cloud,
            reply,
        } = pending;
        let id = TerminalId(data.next_id);
        let opened_session = data.next_session_id();
        let label = self.settings.current.acp.harness_name(preset);
        let node = worker_node(id, &label, preset, Some(group));
        let opened = NodeEdit::Open {
            node: Box::new(node),
            place: NodePlace::Worker {
                session: opened_session,
                spawn,
                cloud,
            },
        };
        self.environments
            .edit(environment.clone(), &opened.into(), None, &mut self.outbox);
        let worker = agent(environment, id);
        let start = ChatStart {
            harness: preset,
            brief: ChatBrief {
                model: None,
                effort: None,
                permission_mode: worker_mode(mode, preset),
                mcp_servers: None,
                role: ChatRole::Agent,
                ..ChatBrief::default()
            },
            prepared: None,
            session: LiveSession::Start(new_session_id()),
            messages: Vec::new(),
            queued: vec![Prompt::from(brief)],
            notices: Vec::new(),
        };
        tracing::info!(worker = id.as_u64(), "a worker started in the runtime");
        self.open_chat(&worker, start);
        self.workers.launching(worker, reply);
    }

    pub(super) fn follow_workers(&mut self) {
        let chats = &self.chats;
        for failed in self.workers.follow(|worker| chats.launch(worker)) {
            self.close_worker(&failed, true);
        }
    }

    pub(super) fn close_worker(&mut self, worker: &AgentRef, forget: bool) {
        self.workers.closed(worker);
        self.chats.close(worker, &mut self.outbox);
        self.terminals.close(worker);
        self.stop_cloud_node(worker);
        let environment = worker.environment.clone();
        if forget {
            let forgotten = NodeEdit::ForgetWorker {
                terminal: worker.terminal,
            };
            self.environments.edit(
                environment.clone(),
                &forgotten.into(),
                None,
                &mut self.outbox,
            );
        }
        let closed = NodeEdit::Close {
            terminal: worker.terminal,
        };
        self.environments
            .edit(environment, &closed.into(), None, &mut self.outbox);
    }

    pub(super) fn is_busy(&self, environment: &EnvironmentName, terminal: TerminalId) -> bool {
        let worker = agent(environment, terminal);
        let working = self
            .environments
            .data(environment)
            .and_then(|data| data.nodes.iter().find(|node| node.id == terminal))
            .is_some_and(|node| {
                matches!(
                    node.state,
                    AgentState::Working | AgentState::WaitingForInput
                )
            });
        working || self.chats.is_busy(&worker) || self.workers.is_launching(&worker)
    }

    fn close_plans(
        &self,
        environment: &EnvironmentName,
        lead: SessionId,
        order: CloseOrder,
    ) -> Result<Vec<ClosePlan>, CloseRefusal> {
        let data = self
            .environments
            .data(environment)
            .ok_or(CloseRefusal::Unknown)?;
        let busy = |terminal| self.is_busy(environment, terminal);
        let subjects = match order.target {
            CloseTarget::Agent(terminal) => vec![CloseSubject::Worker(TerminalId(terminal))],
            CloseTarget::Session(session) => vec![CloseSubject::Session(SessionId(session))],
            CloseTarget::AllIdle => data
                .idle_workers(lead, busy)
                .into_iter()
                .map(CloseSubject::Worker)
                .collect(),
        };
        subjects
            .into_iter()
            .map(|subject| data.plan_close(lead, subject, order.when_working, busy))
            .collect()
    }

    pub(super) fn close_as_lead(&mut self, lead: &LeadHandle, order: CloseOrder) -> LeadClose {
        let environment = EnvironmentName::new(lead.environment.clone());
        let plans = match self.close_plans(&environment, SessionId(lead.session), order) {
            Ok(plans) => plans,
            Err(refusal) => return LeadClose::Refused(refusal),
        };
        let removes = order.worktree == WorktreeFate::Remove
            && plans.iter().any(|plan| plan.worktree.is_some());
        if removes {
            return LeadClose::Removing(plans);
        }
        LeadClose::Closed(
            plans
                .iter()
                .map(|plan| self.close_planned(&environment, plan))
                .collect(),
        )
    }

    pub(super) fn close_planned(
        &mut self,
        environment: &EnvironmentName,
        plan: &ClosePlan,
    ) -> Closed {
        tracing::info!(subject = ?plan.subject, "lead closes in the runtime");
        let worktree = plan.worktree.and_then(|group| {
            let root = self
                .environments
                .data(environment)?
                .group(group)?
                .root
                .clone();
            Some(WorktreeLeft {
                path: root,
                outcome: WorktreeOutcome::Kept,
            })
        });
        if let CloseSubject::Session(session) = plan.subject {
            let archived = if self.settings.current.archive_retention.keeps() {
                SessionEdit::Archive {
                    session,
                    at: SystemTime::now(),
                }
            } else {
                SessionEdit::Leave { session }
            };
            self.environments.edit(
                environment.clone(),
                &archived.into(),
                None,
                &mut self.outbox,
            );
        }
        for terminal in &plan.cancel {
            self.chats.stop_turn(&agent(environment, *terminal));
        }
        for terminal in &plan.terminals {
            self.close_worker(&agent(environment, *terminal), false);
        }
        if let CloseSubject::Session(session) = plan.subject {
            self.environments.edit(
                environment.clone(),
                &SessionEdit::Close { session }.into(),
                None,
                &mut self.outbox,
            );
        }
        Closed {
            target: closed_target(plan.subject),
            worktree,
        }
    }
}
