use futures::future::join_all;
use gpui_kit::Context;
use krad_client::ClientError;
use krad_domain::agents_api::{
    ApiError, CloseOrder, CloseTarget, Closed, ErrorCode, Response, TerminalState, WorktreeLeft,
    WorktreeOutcome,
};
use krad_domain::{
    AgentState, AppEvent, ClosePlan, CloseRefusal, CloseSubject, EnvironmentData, EnvironmentName,
    GroupId, SessionId, TerminalId, WhenWorking, WorktreeFate,
};
use krad_protocol::{Answer, Command, GitAct, GitCommand, RepoAsk, RepoCommand};
use krad_runtime::{closed_target, refusal_error, BusReply};
use std::path::PathBuf;

use super::super::surface::ChatPlace;
use super::super::EnvironmentShell;
use super::{handle, Settlement};

const NOT_ON_SCREEN: &str = "That environment is not the one on screen; open it in the app first";
const WORKTREE_IN_USE: &str = "Other sessions still use that worktree, so it was kept";
const UNEXPECTED_ANSWER: &str = "The runtime answered the removal with something else";
const UNKNOWN_VARIANTS_RUN: &str = "No variants run with that id in your environment";
const FOREIGN_VARIANTS_RUN: &str = "Only the variants runs of your session's group can be picked";

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

fn settled_outcome(answer: Result<Answer, ClientError>) -> WorktreeOutcome {
    match answer {
        Ok(Answer::SettledRemoval { outcome }) => outcome,
        Ok(_) => WorktreeOutcome::Refused(ApiError::new(ErrorCode::Failed, UNEXPECTED_ANSWER)),
        Err(error) => WorktreeOutcome::Refused(ApiError::new(ErrorCode::Failed, error.to_string())),
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
        let settlement = self.settled_state(&handle(env, terminal));
        let working = matches!(
            settlement,
            Settlement::Working
                | Settlement::Settled(TerminalState::Running(AgentState::WaitingForInput))
        );
        working || self.is_launching(env, terminal)
    }

    pub(super) fn close_for_lead(
        &mut self,
        env: &EnvironmentName,
        lead: SessionId,
        order: CloseOrder,
        reply: BusReply,
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
            self.stop_turn_in(&ChatPlace::here(env), *terminal);
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
            if let Some(group) = plan.worktree {
                self.runtime.client.post(Command::Git(GitCommand {
                    environment: env.clone(),
                    group,
                    act: GitAct::AgentsFinished,
                }));
            }
            self.close_planned(env, &plan, cx);
        }
    }

    fn carry_out(
        &mut self,
        env: &EnvironmentName,
        plans: &[ClosePlan],
        fate: WorktreeFate,
        reply: BusReply,
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
        let removing: Vec<_> = removals
            .into_iter()
            .map(|removal| {
                let command = Command::Repo(RepoCommand {
                    environment: env.clone(),
                    ask: RepoAsk::RemoveSettledWorktree {
                        repo: removal.repo_root,
                        path: removal.path,
                    },
                });
                let asked = self.runtime.ask(command, cx);
                async move { (removal.group, settled_outcome(asked.await)) }
            })
            .collect();
        let env = env.clone();
        cx.spawn(async move |this, cx| {
            let removed = join_all(removing).await;
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
        removed: Vec<(GroupId, WorktreeOutcome)>,
        reply: BusReply,
        cx: &mut Context<Self>,
    ) {
        let mut outcomes: Vec<(GroupId, WorktreeOutcome)> = Vec::new();
        for (group, outcome) in removed {
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
