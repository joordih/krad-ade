use futures::channel::oneshot;
use krad_agents::{ErrorCode, Response, TerminalState};
use krad_domain::{
    AgentPreset, AgentState, ChatBookmarks, ChatCheckpoints, ChatChoice, ChatUsage,
    EnvironmentData, GroupId, NodeKind, SessionId, TerminalId, TerminalNode, TextScale,
    WorkerSurface,
};
use krad_protocol::AgentRef;
use std::collections::HashMap;

use crate::chats::{runs_in_runtime, Launch};

mod closing;

pub use closing::{closed_target, refusal_error};

const WORKER_FAILED: &str = "The worker could not start";
const WORKER_CLOSED_EARLY: &str = "The worker was closed before it started";

#[derive(Debug)]
pub(crate) struct WorkerPlan {
    pub(crate) group: GroupId,
    pub(crate) lead_terminal: Option<TerminalId>,
}

#[must_use]
pub(crate) fn plan_spawn(
    data: &EnvironmentData,
    lead: SessionId,
    agent: AgentPreset,
    worktree: Option<&str>,
) -> Option<WorkerPlan> {
    let session = data.session(lead)?;
    let lead_terminal = session.lead.as_ref()?.terminal;
    let group = session.group.filter(|group| data.group(*group).is_some())?;
    let runs_here = worktree.is_none()
        && session.cloud.is_none()
        && agent.acp_agent().is_none()
        && agent.worker_surface() == WorkerSurface::Chat
        && runs_in_runtime(agent);
    runs_here.then_some(WorkerPlan {
        group,
        lead_terminal,
    })
}

#[must_use]
pub(crate) fn worker_node(
    id: TerminalId,
    label: &str,
    agent: AgentPreset,
    group: GroupId,
) -> TerminalNode {
    TerminalNode {
        id,
        title: format!("{label} · {}", id.as_u64()),
        state: AgentState::Idle,
        harness: Some(agent),
        resume: None,
        group: Some(group),
        text_scale: TextScale::default(),
        kind: NodeKind::Chat,
        file: None,
        extension: None,
        bookmarks: ChatBookmarks::default(),
        usage: ChatUsage::default(),
        checkpoints: ChatCheckpoints::default(),
        choice: ChatChoice::default(),
        cloud: None,
    }
}

#[derive(Debug, Default)]
pub(crate) struct Workers {
    launches: HashMap<AgentRef, oneshot::Sender<Response>>,
    waiters: Vec<(AgentRef, oneshot::Sender<Response>)>,
}

impl Workers {
    pub(crate) fn launching(&mut self, worker: AgentRef, reply: oneshot::Sender<Response>) {
        self.launches.insert(worker, reply);
    }

    pub(crate) fn is_launching(&self, worker: &AgentRef) -> bool {
        self.launches.contains_key(worker)
    }

    pub(crate) fn follow(&mut self, launch: impl Fn(&AgentRef) -> Launch) -> Vec<AgentRef> {
        let decided: Vec<(AgentRef, Launch)> = self
            .launches
            .keys()
            .map(|worker| (worker.clone(), launch(worker)))
            .filter(|(_, launch)| *launch != Launch::Pending)
            .collect();
        decided
            .into_iter()
            .filter_map(|(worker, launch)| {
                let reply = self.launches.remove(&worker)?;
                let (response, failed) = match launch {
                    Launch::Started => (
                        Response::Spawned {
                            terminal: worker.terminal.as_u64(),
                        },
                        None,
                    ),
                    Launch::Failed(error) => (
                        Response::error(ErrorCode::Failed, format!("{WORKER_FAILED}: {error}")),
                        Some(worker),
                    ),
                    Launch::Pending => return None,
                };
                if reply.send(response).is_err() {
                    tracing::trace!("the lead left before its worker started");
                }
                failed
            })
            .collect()
    }

    pub(crate) fn wait(&mut self, target: AgentRef, reply: oneshot::Sender<Response>) {
        self.waiters.push((target, reply));
    }

    pub(crate) fn settle(&mut self, settled: impl Fn(&AgentRef) -> Option<TerminalState>) {
        self.waiters.retain(|(_, reply)| !reply.is_canceled());
        let (done, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.waiters)
            .into_iter()
            .map(|(target, reply)| (settled(&target), target, reply))
            .partition(|(state, _, _)| state.is_some());
        self.waiters = waiting
            .into_iter()
            .map(|(_, target, reply)| (target, reply))
            .collect();
        for (state, target, reply) in done {
            let Some(state) = state else {
                continue;
            };
            let response = Response::Settled {
                target: target.terminal.as_u64(),
                state,
            };
            if reply.send(response).is_err() {
                tracing::trace!("a waiter left before its target settled");
            }
        }
    }

    pub(crate) fn closed(&mut self, worker: &AgentRef) {
        let Some(reply) = self.launches.remove(worker) else {
            return;
        };
        let closed = Response::error(ErrorCode::Failed, WORKER_CLOSED_EARLY);
        if reply.send(closed).is_err() {
            tracing::trace!("the lead left before its worker closed");
        }
    }
}
