use futures::channel::oneshot;
use krad_agents::{ApiError, Closed, ErrorCode, Response, WorktreeOutcome};
use krad_domain::{AppEvent, ClosePlan, EnvironmentName};

use super::working::reply_with;
use super::Runtime;
use crate::git::GitJob;
use crate::jobs::JobDone;
use crate::workers::{removal_outcome, remove_settled, LeadRemovals, Removal, WORKTREE_IN_USE};

impl Runtime {
    pub(super) fn close_removing(
        &mut self,
        environment: &EnvironmentName,
        plans: &[ClosePlan],
        reply: oneshot::Sender<Response>,
    ) {
        let mut closed: Vec<Closed> = plans
            .iter()
            .map(|plan| self.close_planned(environment, plan))
            .collect();
        let mut removals: Vec<Removal> = Vec::new();
        let Some(data) = self.environments.data(environment) else {
            reply_with(reply, Response::Closed(closed));
            return;
        };
        for (entry, plan) in closed.iter_mut().zip(plans) {
            let Some(group) = plan.worktree.and_then(|group| data.group(group)) else {
                continue;
            };
            let (Some(origin), Some(left)) = (&group.worktree, entry.worktree.as_mut()) else {
                continue;
            };
            let in_use = data.nodes.iter().any(|node| node.group == Some(group.id));
            if in_use {
                left.outcome =
                    WorktreeOutcome::Refused(ApiError::new(ErrorCode::Busy, WORKTREE_IN_USE));
            } else if removals.iter().all(|removal| removal.group != group.id) {
                removals.push(Removal {
                    group: group.id,
                    repo_root: origin.repo_root.clone(),
                    path: group.root.clone(),
                });
            }
        }
        if removals.is_empty() {
            reply_with(reply, Response::Closed(closed));
            return;
        }
        let link = self.hosts.link(&data.host);
        let environment = environment.clone();
        let started = self.jobs.run(move || {
            let removed = remove_settled(&link, removals);
            JobDone::Git(GitJob::Removals(LeadRemovals {
                environment,
                closed,
                removed,
                reply,
            }))
        });
        if let Err(error) = started {
            tracing::warn!(%error, "could not start removing the worktree");
        }
    }

    pub(super) fn removals_done(&mut self, done: LeadRemovals) {
        let LeadRemovals {
            environment,
            mut closed,
            removed,
            reply,
        } = done;
        for (removal, result) in removed {
            let outcome = removal_outcome(result);
            if outcome == WorktreeOutcome::Removed {
                self.events.publish_own(
                    &environment,
                    Some(removal.group),
                    &AppEvent::WorktreeRemoved,
                    &mut self.outbox,
                );
                self.remove_group(&environment, removal.group);
            }
            for left in closed
                .iter_mut()
                .filter_map(|entry| entry.worktree.as_mut())
                .filter(|left| left.path == removal.path)
            {
                left.outcome = outcome.clone();
            }
        }
        self.git.refresh(&environment);
        reply_with(reply, Response::Closed(closed));
    }
}
