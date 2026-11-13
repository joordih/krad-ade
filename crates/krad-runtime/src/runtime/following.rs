use futures::channel::oneshot;
use krad_agents::{Caller, Incoming, Request, Response, TerminalState};
use krad_chat::LiveSession;
use krad_domain::{
    conversation_lines, AgentState, CloseSubject, EnvironmentName, NodeEdit, NodeField, NodePlace,
    Prompt, ResumeId, SessionId, TerminalId, WhenWorking, WorkerSpawn,
};
use krad_protocol::AgentRef;

use super::Runtime;
use crate::bus::closed_worker_infos;
use crate::chats::{runs_in_runtime, ChatBrief, ChatRole, ChatStart};
use crate::workers::worker_node;

fn reply_with(reply: oneshot::Sender<Response>, response: Response) {
    if reply.send(response).is_err() {
        tracing::trace!("a bus caller left before its answer");
    }
}

fn agent(environment: &EnvironmentName, terminal: TerminalId) -> AgentRef {
    AgentRef {
        environment: environment.clone(),
        terminal,
    }
}

impl Runtime {
    pub(super) fn list_all(&self, incoming: Incoming) -> Option<Incoming> {
        if !matches!(incoming.request, Request::ListAll) {
            return Some(incoming);
        }
        let bus = self.bus.as_ref()?;
        let agents = match &incoming.from {
            Caller::Terminal(terminal) => {
                let environment = EnvironmentName::new(terminal.environment.clone());
                let mut agents = bus.agents_in(&terminal.environment, Some(terminal.terminal));
                agents.extend(
                    self.environments
                        .data(&environment)
                        .map_or_else(Vec::new, closed_worker_infos),
                );
                agents
            }
            Caller::Client => {
                let mut agents = bus.every_agent();
                agents.extend(
                    self.environments
                        .snapshot()
                        .iter()
                        .flat_map(closed_worker_infos),
                );
                agents
            }
            Caller::Lead(_) | Caller::Git(_) => return Some(incoming),
        };
        reply_with(incoming.reply, Response::Agents(agents));
        None
    }

    pub(super) fn answer_on_chats(&mut self, incoming: Incoming) -> Option<Incoming> {
        let (environment, lead) = match &incoming.from {
            Caller::Terminal(terminal) => (terminal.environment.clone(), None),
            Caller::Lead(lead) => (lead.environment.clone(), Some(SessionId(lead.session))),
            Caller::Git(_) | Caller::Client => return Some(incoming),
        };
        let environment = EnvironmentName::new(environment);
        match incoming.request.clone() {
            Request::Wait { target, .. } => {
                self.wait_on_chat(&environment, TerminalId(target), incoming)
            }
            Request::Read { target, lines } => {
                self.read_chat(&environment, lead, (TerminalId(target), lines), incoming)
            }
            Request::Send { to, body } => match lead {
                Some(lead) => {
                    self.send_to_worker(&environment, lead, (TerminalId(to), body), incoming)
                }
                None => Some(incoming),
            },
            _ => Some(incoming),
        }
    }

    pub(super) fn settled_state(&self, target: &AgentRef) -> Option<TerminalState> {
        if self.terminals.knows(target) && !self.chats.is_open(target) {
            return self.terminal_settled(target);
        }
        let state = self
            .environments
            .data(&target.environment)
            .and_then(|data| data.nodes.iter().find(|node| node.id == target.terminal))
            .map_or(AgentState::Idle, |node| node.state);
        let busy = self.chats.is_busy(target)
            || self.workers.is_launching(target)
            || state == AgentState::Working;
        (!busy).then_some(TerminalState::Running(state))
    }

    fn wait_on_chat(
        &mut self,
        environment: &EnvironmentName,
        target: TerminalId,
        incoming: Incoming,
    ) -> Option<Incoming> {
        let target = agent(environment, target);
        if !self.chats.is_open(&target) {
            return Some(incoming);
        }
        match self.settled_state(&target) {
            Some(state) => reply_with(
                incoming.reply,
                Response::Settled {
                    target: target.terminal.as_u64(),
                    state,
                },
            ),
            None => self.workers.wait(target, incoming.reply),
        }
        None
    }

    fn read_chat(
        &mut self,
        environment: &EnvironmentName,
        lead: Option<SessionId>,
        (target, lines): (TerminalId, usize),
        incoming: Incoming,
    ) -> Option<Incoming> {
        let chat = agent(environment, target);
        let Some(messages) = self.chats.tail(&chat) else {
            return Some(incoming);
        };
        let all = conversation_lines(messages);
        let tail = all[all.len().saturating_sub(lines)..].join("\n");
        reply_with(incoming.reply, Response::Screen(tail));
        let finished = matches!(
            self.settled_state(&chat),
            Some(TerminalState::Running(AgentState::Idle | AgentState::Done))
        );
        if let (Some(lead), true) = (lead, finished) {
            self.close_finished_worker(environment, lead, target);
        }
        None
    }

    pub(super) fn close_finished_worker(
        &mut self,
        environment: &EnvironmentName,
        lead: SessionId,
        terminal: TerminalId,
    ) {
        let plan = self.environments.data(environment).and_then(|data| {
            data.plan_close(
                lead,
                CloseSubject::Worker(terminal),
                WhenWorking::Refuse,
                |_| false,
            )
            .ok()
        });
        if let Some(plan) = plan {
            self.close_planned(environment, &plan);
        }
    }

    fn send_to_worker(
        &mut self,
        environment: &EnvironmentName,
        lead: SessionId,
        (target, body): (TerminalId, String),
        incoming: Incoming,
    ) -> Option<Incoming> {
        let chat = agent(environment, target);
        let Some((owner, worker)) = self
            .environments
            .data(environment)
            .and_then(|data| data.worker(target))
        else {
            return Some(incoming);
        };
        if owner != lead {
            return Some(incoming);
        }
        if worker.session.is_some() {
            if !self.chats.is_open(&chat) {
                return Some(incoming);
            }
            self.with_chat_work(|chats, work| chats.send_prompt(&chat, Prompt::from(body), work));
        } else if !self.reopen_worker(environment, lead, target, body) {
            return Some(incoming);
        }
        reply_with(
            incoming.reply,
            Response::Submitted {
                terminal: target.as_u64(),
            },
        );
        None
    }

    fn reopen_worker(
        &mut self,
        environment: &EnvironmentName,
        lead: SessionId,
        terminal: TerminalId,
        body: String,
    ) -> bool {
        let Some(data) = self.environments.data(environment) else {
            return false;
        };
        let Some((_, worker)) = data.worker(terminal) else {
            return false;
        };
        let (Some(reopen), task) = (worker.reopen.clone(), worker.task.clone()) else {
            return false;
        };
        let local = data
            .session(lead)
            .is_some_and(|session| session.cloud.is_none());
        let runs_here =
            local && reopen.harness.acp_agent().is_none() && runs_in_runtime(reopen.harness);
        let Some(group) = reopen
            .group
            .filter(|group| runs_here && data.group(*group).is_some())
        else {
            return false;
        };
        let session = data.next_session_id();
        let label = self.settings.current.acp.harness_name(reopen.harness);
        let opened = NodeEdit::Open {
            node: Box::new(worker_node(terminal, &label, reopen.harness, Some(group))),
            place: NodePlace::Worker {
                session,
                spawn: WorkerSpawn {
                    lead: Some(lead),
                    task,
                },
                cloud: None,
            },
        };
        let resumed = NodeEdit::ResumeWorker {
            terminal,
            resume: reopen.resume.clone(),
        };
        for edit in [opened, resumed] {
            self.environments
                .edit(environment.clone(), &edit.into(), None, &mut self.outbox);
        }
        let start = ChatStart {
            harness: reopen.harness,
            brief: ChatBrief {
                role: ChatRole::Agent,
                ..ChatBrief::default()
            },
            prepared: None,
            session: LiveSession::Resume(reopen.resume),
            messages: Vec::new(),
            queued: vec![Prompt::from(body)],
            notices: Vec::new(),
        };
        tracing::info!(
            worker = terminal.as_u64(),
            "a lead reopened its worker in the runtime"
        );
        self.open_chat(&agent(environment, terminal), start);
        true
    }

    pub(super) fn keep_chat_resumes(&mut self) {
        let moved: Vec<(AgentRef, ResumeId)> = self
            .chats
            .resumes()
            .filter(|(chat, resume)| {
                self.environments
                    .data(&chat.environment)
                    .and_then(|data| data.nodes.iter().find(|node| node.id == chat.terminal))
                    .is_some_and(|node| node.resume.as_ref() != Some(*resume))
            })
            .map(|(chat, resume)| (chat.clone(), resume.clone()))
            .collect();
        for (chat, resume) in moved {
            let kept = NodeEdit::Set {
                terminal: chat.terminal,
                field: NodeField::Resume(Some(resume)),
            };
            self.environments
                .edit(chat.environment, &kept.into(), None, &mut self.outbox);
        }
    }

    pub(super) fn settle_waiters(&mut self) {
        let mut workers = std::mem::take(&mut self.workers);
        workers.settle(|target| self.settled_state(target));
        self.workers = workers;
    }
}
