use krad_domain::AppEvent;
use krad_protocol::Reply;
use krad_repo::RepoError;
use std::path::PathBuf;

use super::kraddie_runs::RunPlan;
use super::Runtime;
use crate::git::{Act, ActOrder, Acted, Asker, AskpassPlan};

const NO_WORKTREE_GROUP: &str = "The worktree was made but no group opened for it";

impl Runtime {
    #[tracing::instrument(skip_all, fields(kraddie = %plan.owner.kraddie(), run = plan.run.0))]
    pub(super) fn open_kraddie_worktree(&mut self, plan: RunPlan) {
        let environment = plan.workspace.environment.clone();
        let Some(host) = self.environments.host_of(&environment) else {
            return self.fail_kraddie_run(&plan, NO_WORKTREE_GROUP);
        };
        let from = self.repo_group(&environment, &plan.workspace.repo);
        let order = ActOrder {
            environment,
            group: from,
            link: self.hosts.link(&host),
            repo: plan.workspace.repo.clone(),
            askpass: AskpassPlan::Off,
            act: Act::AddWorktree(plan.branch.clone()),
            asker: Asker::Kraddie(Box::new(plan)),
        };
        if let Reply::Refused { message, .. } = self.start_act(order) {
            tracing::warn!(%message, "a Kraddie run's worktree was not started");
        }
    }

    pub(super) fn kraddie_worktree_made(
        &mut self,
        plan: &RunPlan,
        repo: PathBuf,
        acted: Result<Acted, RepoError>,
    ) {
        let worktree = match acted {
            Ok(Acted::Created(worktree)) => worktree,
            Ok(_) => return self.fail_kraddie_run(plan, NO_WORKTREE_GROUP),
            Err(error) => return self.fail_kraddie_run(plan, &error.to_string()),
        };
        let environment = plan.workspace.environment.clone();
        let Some(group) = self.open_worktree_group(&environment, repo, worktree) else {
            return self.fail_kraddie_run(plan, NO_WORKTREE_GROUP);
        };
        self.kraddies.groups.insert((environment.clone(), group));
        if let Some(from) = self.repo_group(&environment, &plan.workspace.repo) {
            self.events.publish_own(
                &environment,
                Some(group),
                &AppEvent::WorktreeCreated { from },
                &mut self.outbox,
            );
        }
        self.open_kraddie_session(plan, group);
    }
}
