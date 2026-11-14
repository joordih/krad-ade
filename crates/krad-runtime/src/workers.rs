use futures::channel::oneshot;
use krad_agents::{ErrorCode, Response};
use krad_domain::{
    AgentPreset, AgentState, ChatBookmarks, ChatCheckpoints, ChatChoice, ChatUsage, GroupId,
    NodeKind, TerminalId, TerminalNode, TextScale, WorkerSurface,
};
use krad_protocol::AgentRef;
use std::collections::HashMap;

use crate::chats::{runs_in_runtime, Launch};

mod closing;
mod removal;
mod spawning;

pub use closing::{closed_target, refusal_error};
pub use removal::removal_outcome;
pub(crate) use removal::{remove_settled, LeadRemovals, Removal, WORKTREE_IN_USE};
pub(crate) use spawning::{plan_worker, PendingWorker, SpawnPlan, WorkerAsker, WorkerOrder};

const WORKER_FAILED: &str = "The worker could not start";
const WORKER_CLOSED_EARLY: &str = "The worker was closed before it started";

#[must_use]
pub(crate) fn runs_as_worker(agent: AgentPreset) -> bool {
    agent.worker_surface() == WorkerSurface::Chat && runs_in_runtime(agent)
}

#[must_use]
pub(crate) fn worker_node(
    id: TerminalId,
    label: &str,
    agent: AgentPreset,
    group: Option<GroupId>,
) -> TerminalNode {
    TerminalNode {
        id,
        title: format!("{label} · {}", id.as_u64()),
        state: AgentState::Idle,
        harness: Some(agent),
        resume: None,
        group,
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

    pub(crate) fn settle(&mut self, answer: impl Fn(&AgentRef) -> Option<Response>) {
        self.waiters.retain(|(_, reply)| !reply.is_canceled());
        let (done, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.waiters)
            .into_iter()
            .map(|(target, reply)| (answer(&target), target, reply))
            .partition(|(response, _, _)| response.is_some());
        self.waiters = waiting
            .into_iter()
            .map(|(_, target, reply)| (target, reply))
            .collect();
        for (response, _, reply) in done {
            let Some(response) = response else {
                continue;
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
