use krad_agents::{ErrorCode, Response};
use krad_domain::{sanitize_worktree_name, AppEvent, BaseFetch, EnvironmentName, GroupId};
use krad_protocol::Reply;
use krad_repo::RepoError;
use std::path::PathBuf;

use super::working::reply_with;
use super::Runtime;
use crate::git::{Act, ActOrder, Acted, Asker, AskpassPlan};
use crate::workers::PendingWorker;

const NOT_A_REPO: &str =
    "The caller's group is not in a git repository, so it cannot branch a worktree";
const BAD_WORKTREE_NAME: &str = "Use letters, digits, '-', '_' or '.' for the worktree name";
const NO_WORKTREE_GROUP: &str = "The worktree was made but no group opened for it";

impl Runtime {
    pub(super) fn spawn_in_worktree(
        &mut self,
        environment: &EnvironmentName,
        group: GroupId,
        name: &str,
        pending: PendingWorker,
    ) {
        let Some(name) = sanitize_worktree_name(name) else {
            reply_with(
                pending.reply,
                Response::error(ErrorCode::BadRequest, BAD_WORKTREE_NAME),
            );
            return;
        };
        let Some(data) = self.environments.data(environment) else {
            return;
        };
        let host = data.host.clone();
        let Some(repo) = data
            .group(group)
            .and_then(|found| self.git.at(&host, &found.root))
            .map(|git| git.repo_root.clone())
        else {
            reply_with(
                pending.reply,
                Response::error(ErrorCode::Unsupported, NOT_A_REPO),
            );
            return;
        };
        let settings = self.settings.current.worktrees.clone();
        let askpass = if settings.fetch == BaseFetch::FetchFirst {
            self.git_askpass(environment, &host, &repo)
        } else {
            AskpassPlan::Off
        };
        let order = ActOrder {
            asker: Asker::Worker(Box::new(pending)),
            environment: environment.clone(),
            group: Some(group),
            link: self.hosts.link(&host),
            repo,
            askpass,
            act: Act::CreateWorktree { name, settings },
        };
        if let Reply::Refused { message, .. } = self.start_act(order) {
            tracing::warn!(%message, "a worker's worktree was not started");
        }
    }

    pub(super) fn worker_worktree_made(
        &mut self,
        environment: &EnvironmentName,
        pending: PendingWorker,
        from: Option<GroupId>,
        (repo, acted): (PathBuf, Result<Acted, RepoError>),
    ) {
        let worktree = match acted {
            Ok(Acted::Created(worktree)) => worktree,
            Ok(_) => {
                reply_with(
                    pending.reply,
                    Response::error(ErrorCode::Failed, NO_WORKTREE_GROUP),
                );
                return;
            }
            Err(error) => {
                reply_with(
                    pending.reply,
                    Response::error(ErrorCode::Failed, error.to_string()),
                );
                return;
            }
        };
        let Some(opened) = self.open_worktree_group(environment, repo, worktree) else {
            reply_with(
                pending.reply,
                Response::error(ErrorCode::Failed, NO_WORKTREE_GROUP),
            );
            return;
        };
        if let Some(from) = from {
            self.events.publish_own(
                environment,
                Some(opened),
                &AppEvent::WorktreeCreated { from },
                &mut self.outbox,
            );
        }
        self.open_worker(environment, pending, opened);
    }
}
