use futures::channel::oneshot::Sender as ReplySender;
use gpui_kit::{AppContext as _, Context};
use krad_agents::{
    AgentInfo, ApiError, CloseOrder, CloseTarget, Closed, ClosedTarget, ErrorCode, Response,
    TerminalState, WorktreeFate, WorktreeLeft, WorktreeOutcome,
};
use krad_domain::{
    worker_session_name, AgentState, AppEvent, ClosePlan, CloseRefusal, CloseSubject,
    EnvironmentData, EnvironmentName, GroupId, SessionId, TerminalId, WhenWorking,
};
use krad_repo::{remove_settled_worktree, RepoError};
use std::path::PathBuf;

use super::super::EnvironmentShell;
use super::{handle, Settlement};

const UNKNOWN_TARGET: &str = "No worker or session with that id in your environment";
const ALREADY_CLOSED: &str = "That worker is already closed; send it a message to reopen it";
const NOT_OWNED: &str = "Only the workers you started and the sessions of your group can be closed";
const ITSELF: &str = "A lead cannot close itself or the session it runs in";
const WORKING: &str = "It is still working; pass --force to stop its turn and close it";
const NOT_ON_SCREEN: &str = "That environment is not the one on screen; open it in the app first";
const WORKTREE_IN_USE: &str = "Other sessions still use that worktree, so it was kept";
const UNKNOWN_VARIANTS_RUN: &str = "No variants run with that id in your environment";
const FOREIGN_VARIANTS_RUN: &str = "Only the variants runs of your session's group can be picked";

#[must_use]
pub(super) fn refusal_error(refusal: CloseRefusal) -> ApiError {
    let (code, message) = match refusal {
        CloseRefusal::Unknown => (ErrorCode::NotFound, UNKNOWN_TARGET),
        CloseRefusal::AlreadyClosed => (ErrorCode::NotFound, ALREADY_CLOSED),
        CloseRefusal::NotOwned => (ErrorCode::NotOwned, NOT_OWNED),
        CloseRefusal::Itself => (ErrorCode::Forbidden, ITSELF),
        CloseRefusal::Working => (ErrorCode::Busy, WORKING),
    };
    ApiError::new(code, message)
}

#[must_use]
pub(super) fn session_close_refusal(
    data: &EnvironmentData,
    lead: SessionId,
    session: SessionId,
) -> Option<ApiError> {
    data.plan_close(
        lead,
        CloseSubject::Session(session),
        WhenWorking::CancelTurn,
        |_| false,
    )
    .err()
    .map(refusal_error)
}

#[must_use]
pub(super) fn pick_refusal(run_group: Option<GroupId>, lead_group: GroupId) -> Option<ApiError> {
    match run_group {
        None => Some(ApiError::new(ErrorCode::NotFound, UNKNOWN_VARIANTS_RUN)),
        Some(group) if group != lead_group => {
            Some(ApiError::new(ErrorCode::NotOwned, FOREIGN_VARIANTS_RUN))
        }
        Some(_) => None,
    }
}

#[must_use]
pub(super) fn removal_outcome(removed: Result<(), RepoError>) -> WorktreeOutcome {
    match removed {
        Ok(()) => WorktreeOutcome::Removed,
        Err(error @ (RepoError::UncommittedChanges | RepoError::UnmergedCommits(_))) => {
            WorktreeOutcome::Refused(ApiError::new(
                ErrorCode::WorktreeNotClean,
                error.to_string(),
            ))
        }
        Err(error) => WorktreeOutcome::Refused(ApiError::new(ErrorCode::Failed, error.to_string())),
    }
}

#[must_use]
pub(super) fn closed_worker_infos(data: &EnvironmentData) -> Vec<AgentInfo> {
    data.sessions
        .iter()
        .filter_map(|session| session.lead.as_ref())
        .flat_map(|lead| &lead.workers)
        .filter(|worker| worker.session.is_none())
        .map(|worker| AgentInfo {
            environment: Some(data.name.as_str().to_owned()),
            terminal: worker.terminal.as_u64(),
            title: worker_session_name(&worker.task),
            group: worker
                .reopen
                .as_ref()
                .and_then(|reopen| reopen.group)
                .and_then(|group| data.group(group))
                .map(|group| group.name.clone()),
            state: TerminalState::Closed,
            harness: worker.reopen.as_ref().map(|reopen| reopen.harness),
            is_self: false,
        })
        .collect()
}

const fn closed_target(subject: CloseSubject) -> ClosedTarget {
    match subject {
        CloseSubject::Worker(terminal) => ClosedTarget::Agent(terminal.as_u64()),
        CloseSubject::Session(session) => ClosedTarget::Session(session.as_u64()),
    }
}

struct Removal {
    group: GroupId,
    repo_root: PathBuf,
    path: PathBuf,
}

struct ClosedEntry {
    closed: Closed,
    removing: Option<GroupId>,
}

impl EnvironmentShell {
    pub(super) fn is_busy(&self, env: &EnvironmentName, terminal: TerminalId) -> bool {
        let target = handle(env, terminal);
        let starting = self
            .agent_bus
            .as_ref()
            .is_some_and(|bus| bus.is_starting(&target));
        let settlement = self.settled_state(&target);
        let working = matches!(
            settlement,
            Settlement::Working
                | Settlement::Settled(TerminalState::Running(AgentState::WaitingForInput))
        );
        starting || working || self.is_launching(env, terminal)
    }

    pub(super) fn close_for_lead(
        &mut self,
        env: &EnvironmentName,
        lead: SessionId,
        order: CloseOrder,
        reply: ReplySender<Response>,
        cx: &mut Context<Self>,
    ) {
        match self.close_plans(env, lead, order) {
            Ok(plans) => self.carry_out(env, &plans, order.worktree, reply, cx),
            Err(error) => reply.send(Response::Error(error)).unwrap_or_default(),
        }
    }

    fn close_plans(
        &self,
        env: &EnvironmentName,
        lead: SessionId,
        order: CloseOrder,
    ) -> Result<Vec<ClosePlan>, ApiError> {
        let Some(live) = self.envs.get(env) else {
            return Err(refusal_error(CloseRefusal::Unknown));
        };
        let busy = |terminal| self.is_busy(env, terminal);
        let subjects = match order.target {
            CloseTarget::Agent(terminal) => vec![CloseSubject::Worker(TerminalId(terminal))],
            CloseTarget::Session(_) if *env != self.active => {
                return Err(ApiError::new(ErrorCode::Unsupported, NOT_ON_SCREEN));
            }
            CloseTarget::Session(session) => vec![CloseSubject::Session(SessionId(session))],
            CloseTarget::AllIdle => live
                .data
                .idle_workers(lead, busy)
                .into_iter()
                .map(CloseSubject::Worker)
                .collect(),
        };
        subjects
            .into_iter()
            .map(|subject| {
                live.data
                    .plan_close(lead, subject, order.when_working, busy)
                    .map_err(refusal_error)
            })
            .collect()
    }

    pub(super) fn close_planned(
        &mut self,
        env: &EnvironmentName,
        plan: &ClosePlan,
        cx: &mut Context<Self>,
    ) {
        for terminal in &plan.cancel {
            self.stop_turn_in(env, *terminal);
        }
        for terminal in &plan.terminals {
            self.close_terminal_in(env, *terminal, cx);
        }
        let CloseSubject::Session(session) = plan.subject else {
            return;
        };
        let still_open = self
            .envs
            .get(env)
            .is_some_and(|live| live.data.session(session).is_some());
        if still_open && *env == self.active {
            self.close_session(session, cx);
        }
    }

    pub(super) fn close_finished_worker(
        &mut self,
        env: &EnvironmentName,
        lead: SessionId,
        terminal: TerminalId,
        cx: &mut Context<Self>,
    ) {
        let plan = self.envs.get(env).and_then(|live| {
            let subject = CloseSubject::Worker(terminal);
            live.data
                .plan_close(lead, subject, WhenWorking::Refuse, |_| false)
                .ok()
        });
        if let Some(plan) = plan {
            self.note_agent_finished_in(env, plan.worktree);
            self.close_planned(env, &plan, cx);
            self.sweep_merged_worktrees(cx);
        }
    }

    fn carry_out(
        &mut self,
        env: &EnvironmentName,
        plans: &[ClosePlan],
        fate: WorktreeFate,
        reply: ReplySender<Response>,
        cx: &mut Context<Self>,
    ) {
        for plan in plans {
            tracing::info!(subject = ?plan.subject, "lead closes");
            self.close_planned(env, plan, cx);
        }
        let mut removals: Vec<Removal> = Vec::new();
        let entries: Vec<ClosedEntry> = plans
            .iter()
            .map(|plan| self.closed_entry(env, plan, fate, &mut removals))
            .collect();
        if removals.is_empty() {
            let closed = entries.into_iter().map(|entry| entry.closed).collect();
            reply.send(Response::Closed(closed)).unwrap_or_default();
            return;
        }
        let link = self.env_link(env);
        let removing = cx.background_spawn(async move {
            removals
                .into_iter()
                .map(|removal| {
                    let removed = remove_settled_worktree(&link, &removal.repo_root, &removal.path);
                    (removal.group, removed)
                })
                .collect::<Vec<_>>()
        });
        let env = env.clone();
        cx.spawn(async move |this, cx| {
            let removed = removing.await;
            this.update(cx, |shell, cx| {
                shell.finish_removals(&env, entries, removed, reply, cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn closed_entry(
        &self,
        env: &EnvironmentName,
        plan: &ClosePlan,
        fate: WorktreeFate,
        removals: &mut Vec<Removal>,
    ) -> ClosedEntry {
        let target = closed_target(plan.subject);
        let group = plan
            .worktree
            .and_then(|group| self.envs.get(env)?.data.group(group).cloned());
        let Some((group, origin)) =
            group.and_then(|group| group.worktree.clone().map(|origin| (group, origin)))
        else {
            return ClosedEntry {
                closed: Closed {
                    target,
                    worktree: None,
                },
                removing: None,
            };
        };
        let in_use = self.envs.get(env).is_some_and(|live| {
            live.data
                .nodes
                .iter()
                .any(|node| node.group == Some(group.id))
        });
        let outcome = match fate {
            WorktreeFate::Keep => WorktreeOutcome::Kept,
            WorktreeFate::Remove if in_use => {
                WorktreeOutcome::Refused(ApiError::new(ErrorCode::Busy, WORKTREE_IN_USE))
            }
            WorktreeFate::Remove => {
                if removals.iter().all(|removal| removal.group != group.id) {
                    removals.push(Removal {
                        group: group.id,
                        repo_root: origin.repo_root,
                        path: group.root.clone(),
                    });
                }
                WorktreeOutcome::Kept
            }
        };
        let removing = (fate == WorktreeFate::Remove && !in_use).then_some(group.id);
        ClosedEntry {
            closed: Closed {
                target,
                worktree: Some(WorktreeLeft {
                    path: group.root,
                    outcome,
                }),
            },
            removing,
        }
    }

    fn finish_removals(
        &mut self,
        env: &EnvironmentName,
        entries: Vec<ClosedEntry>,
        removed: Vec<(GroupId, Result<(), RepoError>)>,
        reply: ReplySender<Response>,
        cx: &mut Context<Self>,
    ) {
        let mut outcomes: Vec<(GroupId, WorktreeOutcome)> = Vec::new();
        for (group, result) in removed {
            let outcome = removal_outcome(result);
            if outcome == WorktreeOutcome::Removed {
                self.publish_event(env, Some(group), AppEvent::WorktreeRemoved);
                self.remove_group_in(env, group, cx);
            }
            outcomes.push((group, outcome));
        }
        let closed = entries
            .into_iter()
            .map(|mut entry| {
                let outcome = entry.removing.and_then(|group| {
                    outcomes
                        .iter()
                        .find(|(removed, _)| *removed == group)
                        .map(|(_, outcome)| outcome.clone())
                });
                if let (Some(outcome), Some(worktree)) = (outcome, entry.closed.worktree.as_mut()) {
                    worktree.outcome = outcome;
                }
                entry.closed
            })
            .collect();
        reply.send(Response::Closed(closed)).unwrap_or_default();
    }
}

#[cfg(test)]
mod tests;
