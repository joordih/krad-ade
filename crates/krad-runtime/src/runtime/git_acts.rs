use krad_domain::{
    AppEvent, BaseFetch, CommitMode, EnvironmentName, GroupEdit, GroupGit, GroupId, SessionEdit,
    TerminalGroup, Worktree, WorktreeOrigin, WorktreeSettings,
};
use krad_protocol::{
    ErrorCode, EventKind, GitAct, GitCommand, GitFinished, GitOutcome, Reply, RequestId,
};
use krad_repo::sanitize_worktree_name;
use std::path::PathBuf;

use super::working::agent;
use super::{refused, Runtime, UNSUPPORTED_COMMAND};
use crate::environments::UNKNOWN_ENVIRONMENT;
use crate::git::{run_act, Act, ActDone, ActOrder, Acted, Asker, AskpassPlan, GitJob};
use crate::jobs::JobDone;

const UNKNOWN_GROUP: &str = "this environment has no group of that id";
const NOT_A_REPO: &str = "The group is not inside a git repository";
const DETACHED_PUSH: &str = "Check out a branch before pushing";
const EMPTY_MESSAGE: &str = "Type a commit message first";
const BAD_WORKTREE_NAME: &str = "Use letters, digits, '-', '_' or '.' for the worktree name";
const NOT_A_WORKTREE: &str = "The group is not a worktree created here";
const NOT_STARTED: &str = "Could not start the git command";

impl Runtime {
    pub(super) fn execute_git(&mut self, request: RequestId, command: GitCommand) -> Reply {
        let GitCommand {
            environment,
            group,
            act,
        } = command;
        let Some(data) = self.environments.data(&environment) else {
            return refused(ErrorCode::NotFound, UNKNOWN_ENVIRONMENT);
        };
        let Some(found) = data.group(group).cloned() else {
            return refused(ErrorCode::NotFound, UNKNOWN_GROUP);
        };
        let host = data.host.clone();
        if act == GitAct::AgentsFinished {
            self.git.sweep.note_finished(&environment, group);
        }
        if matches!(act, GitAct::Refresh | GitAct::AgentsFinished) {
            self.git.refresh(&environment);
            return Reply::Accepted;
        }
        if act == GitAct::RemoveWorktree {
            return self.remove_worktree(request, &environment, &found);
        }
        let Some(git) = self.git.at(&host, &found.root) else {
            return refused(ErrorCode::NotFound, NOT_A_REPO);
        };
        let repo = git.repo_root.clone();
        let (act, network) = match resolve(act, git, &self.settings.current.worktrees) {
            Ok(resolved) => resolved,
            Err(refusal) => return refusal,
        };
        let askpass = if network {
            self.git_askpass(&environment, &host, &repo)
        } else {
            AskpassPlan::Off
        };
        let order = ActOrder {
            asker: Asker::Client(request),
            environment,
            group: Some(group),
            link: self.hosts.link(&host),
            repo,
            askpass,
            act,
        };
        self.start_act(order)
    }

    pub(super) fn start_act(&mut self, order: ActOrder) -> Reply {
        match self
            .jobs
            .run(move || JobDone::Git(GitJob::Acted(run_act(order))))
        {
            Ok(()) => Reply::Accepted,
            Err(error) => {
                tracing::warn!(%error, "could not start a git command");
                refused(ErrorCode::Unsupported, NOT_STARTED)
            }
        }
    }

    fn remove_worktree(
        &mut self,
        request: RequestId,
        environment: &EnvironmentName,
        group: &TerminalGroup,
    ) -> Reply {
        let Some(origin) = group.worktree.clone() else {
            return refused(ErrorCode::Invalid, NOT_A_WORKTREE);
        };
        self.close_group_nodes(environment, group.id);
        let Some(host) = self.environments.host_of(environment) else {
            return refused(ErrorCode::NotFound, UNKNOWN_ENVIRONMENT);
        };
        let order = ActOrder {
            asker: Asker::Client(request),
            environment: environment.clone(),
            group: Some(group.id),
            link: self.hosts.link(&host),
            repo: origin.repo_root,
            askpass: AskpassPlan::Off,
            act: Act::RemoveWorktree {
                path: group.root.clone(),
            },
        };
        self.start_act(order)
    }

    pub(super) fn git_acted(&mut self, done: ActDone) {
        let ActDone {
            asker,
            environment,
            group,
            repo,
            acted,
        } = done;
        let request = match asker {
            Asker::Client(request) => request,
            Asker::Worker(pending) => {
                self.worker_worktree_made(&environment, *pending, group, (repo, acted));
                return;
            }
            Asker::Bus(reply) => {
                self.bus_worktree_made(&environment, group, (repo, acted), reply);
                return;
            }
            Asker::Kraddie(plan) => {
                self.kraddie_worktree_made(&plan, repo, acted);
                return;
            }
            Asker::CiFix(pending) => {
                self.fix_worktree_made(*pending, repo, acted);
                return;
            }
        };
        let outcome = match acted {
            Ok(Acted::Done) => GitOutcome::Done,
            Ok(Acted::Stale) => GitOutcome::Stale,
            Ok(Acted::Created(worktree)) => {
                let opened = self.open_worktree_group(&environment, repo, worktree);
                if let (Some(from), Some(opened)) = (group, opened) {
                    self.events.publish_own(
                        &environment,
                        Some(opened),
                        &AppEvent::WorktreeCreated { from },
                        &mut self.outbox,
                    );
                }
                opened.map_or(GitOutcome::Done, |group| GitOutcome::Opened { group })
            }
            Ok(Acted::Opened(worktree)) => self
                .open_worktree_group(&environment, repo, worktree)
                .map_or(GitOutcome::Done, |group| GitOutcome::Opened { group }),
            Ok(Acted::Removed) => {
                if let Some(group) = group {
                    self.events.publish_own(
                        &environment,
                        Some(group),
                        &AppEvent::WorktreeRemoved,
                        &mut self.outbox,
                    );
                    self.remove_group(&environment, group);
                }
                GitOutcome::Done
            }
            Err(error) => GitOutcome::Failed {
                message: error.to_string(),
            },
        };
        self.git.refresh(&environment);
        self.outbox.push(EventKind::GitFinished(GitFinished {
            request,
            environment,
            outcome,
        }));
    }

    pub(super) fn open_worktree_group(
        &mut self,
        environment: &EnvironmentName,
        repo_root: PathBuf,
        worktree: Worktree,
    ) -> Option<GroupId> {
        let data = self.environments.data(environment)?;
        if let Some(open) = data.groups.iter().find(|group| group.root == worktree.path) {
            return Some(open.id);
        }
        let group = data.next_group_id();
        let origin = worktree
            .branch
            .map(|branch| WorktreeOrigin { repo_root, branch });
        let added = GroupEdit::Add {
            group,
            root: worktree.path,
            worktree: origin,
        };
        self.environments
            .edit(environment.clone(), &added.into(), None, &mut self.outbox);
        self.git.refresh(environment);
        Some(group)
    }

    fn close_group_nodes(&mut self, environment: &EnvironmentName, group: GroupId) {
        let held: Vec<_> = self
            .environments
            .data(environment)
            .map_or_else(Vec::new, |data| {
                data.nodes
                    .iter()
                    .filter(|node| node.group == Some(group))
                    .map(|node| node.id)
                    .collect()
            });
        for terminal in held {
            self.close_worker(&agent(environment, terminal), false);
        }
    }

    pub(super) fn remove_group(&mut self, environment: &EnvironmentName, group: GroupId) {
        self.close_group_nodes(environment, group);
        for edit in [
            SessionEdit::ForgetGroupArchive { group }.into(),
            GroupEdit::Remove { group }.into(),
        ] {
            self.environments
                .edit(environment.clone(), &edit, None, &mut self.outbox);
        }
    }
}

fn resolve(
    act: GitAct,
    git: &GroupGit,
    worktrees: &WorktreeSettings,
) -> Result<(Act, bool), Reply> {
    let pushed = || {
        git.push_target()
            .ok_or_else(|| refused(ErrorCode::Invalid, DETACHED_PUSH))
    };
    let resolved = match act {
        GitAct::Fetch => (Act::Fetch, true),
        GitAct::Pull => (Act::Pull, true),
        GitAct::Push => (Act::Push(pushed()?), true),
        GitAct::Sync => (Act::Sync(pushed()?), true),
        GitAct::PushTags => (Act::PushTags, true),
        GitAct::Stash => (Act::Stash, false),
        GitAct::PopStash => (Act::PopStash, false),
        GitAct::Stage { paths } => (Act::Stage(paths), false),
        GitAct::Unstage { paths } => (Act::Unstage(paths), false),
        GitAct::StageAll => (Act::StageAll, false),
        GitAct::UnstageAll => (Act::UnstageAll, false),
        GitAct::Commit {
            message,
            mode,
            push,
        } => {
            if message.trim().is_empty() && mode == CommitMode::New {
                return Err(refused(ErrorCode::Invalid, EMPTY_MESSAGE));
            }
            let push = if push { Some(pushed()?) } else { None };
            let network = push.is_some();
            let commit = Act::Commit {
                message,
                mode,
                scope: git.commit_scope(),
                push,
            };
            (commit, network)
        }
        GitAct::CreateBranch { name } => (Act::CreateBranch(name), false),
        GitAct::CreateTag { name } => (Act::CreateTag(name), false),
        GitAct::DeleteTag { name } => (Act::DeleteTag(name), false),
        GitAct::CheckOut { branch } => (Act::CheckOut(branch), false),
        GitAct::Discard { change } => (Act::Discard(change), false),
        GitAct::DiscardFolder { folder } => (Act::DiscardFolder(folder), false),
        GitAct::CreateWorktree { name } => {
            let name = sanitize_worktree_name(&name)
                .ok_or_else(|| refused(ErrorCode::Invalid, BAD_WORKTREE_NAME))?;
            let network = worktrees.fetch == BaseFetch::FetchFirst;
            let settings = worktrees.clone();
            (Act::CreateWorktree { name, settings }, network)
        }
        GitAct::OpenBranchWorktree { branch } => (Act::OpenBranchWorktree(branch), false),
        GitAct::Refresh | GitAct::AgentsFinished | GitAct::RemoveWorktree | GitAct::Unknown => {
            return Err(refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND));
        }
    };
    Ok(resolved)
}
