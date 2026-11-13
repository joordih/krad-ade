use krad_agents::{
    ApiError, Caller, ChangeSet, ChangedFile, ErrorCode, Incoming, LeadHandle, Request, Response,
    TerminalHandle, WorktreeInfo,
};
use krad_domain::{
    sanitize_worktree_name, AppEvent, BaseFetch, EnvironmentName, GroupId, SessionId, TerminalId,
};
use krad_host::HostLink;
use krad_repo::{status, working_patch, RepoError};
use std::path::{Path, PathBuf};

use super::working::reply_with;
use super::Runtime;
use crate::git::{Act, ActOrder, Acted, Asker, AskpassPlan, BusAnswer, GitJob};
use crate::jobs::JobDone;

const NO_ENVIRONMENT: &str = "No environment of that name in this app";
const NO_CALLER_GROUP: &str = "The caller belongs to no group; name one with --group";
const NO_SUCH_SESSION: &str = "No session with that id in your environment";
const NO_SESSION_GROUP: &str = "That session belongs to no group";
const NOT_A_REPO: &str =
    "The caller's group is not in a git repository, so it cannot branch a worktree";
const BAD_WORKTREE_NAME: &str = "Use letters, digits, '-', '_' or '.' for the worktree name";
const NO_WORKTREE_GROUP: &str = "The worktree was made but no group opened for it";

enum Own {
    Terminal(TerminalId),
    Session(SessionId),
}

struct Asked {
    environment: EnvironmentName,
    group: GroupId,
}

fn refusal(code: ErrorCode, message: &str) -> ApiError {
    ApiError::new(code, message)
}

#[tracing::instrument(skip_all)]
fn read_changes(link: &HostLink, root: &Path, group: String) -> Result<ChangeSet, RepoError> {
    let read = status(link, root)?;
    let patch = working_patch(link, root)?;
    Ok(ChangeSet {
        group,
        root: root.to_path_buf(),
        branch: read.branch,
        files: read
            .changes
            .into_iter()
            .map(|change| ChangedFile {
                path: change.path,
                kind: change.kind,
            })
            .collect(),
        patch,
    })
}

impl Runtime {
    pub(super) fn answer_git_requests(&mut self, incoming: Incoming) -> Option<Incoming> {
        let answered = match &incoming.request {
            Request::CreateWorktree { name } => self
                .caller_group(&incoming.from, None)?
                .map(|asked| self.worktree_for_bus(asked, name)),
            Request::Changes { session } => self
                .caller_group(&incoming.from, *session)?
                .map(|asked| self.changes_for_bus(&asked)),
            _ => return Some(incoming),
        };
        match answered {
            Ok(Ok(order)) => self.start_bus_job(order, incoming.reply),
            Ok(Err(error)) | Err(error) => reply_with(incoming.reply, Response::Error(error)),
        }
        None
    }

    fn caller_group(&self, from: &Caller, session: Option<u64>) -> Option<Result<Asked, ApiError>> {
        let (environment, own) = match from {
            Caller::Terminal(TerminalHandle {
                environment,
                terminal,
            }) => (environment, Own::Terminal(TerminalId(*terminal))),
            Caller::Lead(LeadHandle {
                environment,
                session,
            }) => (environment, Own::Session(SessionId(*session))),
            Caller::Git(_) | Caller::Client => return None,
        };
        let environment = EnvironmentName::new(environment.clone());
        let Some(data) = self.environments.data(&environment) else {
            return Some(Err(refusal(ErrorCode::NotFound, NO_ENVIRONMENT)));
        };
        let session_group = |session| {
            data.session(session)
                .ok_or_else(|| refusal(ErrorCode::NotFound, NO_SUCH_SESSION))?
                .group
                .ok_or_else(|| refusal(ErrorCode::Unsupported, NO_SESSION_GROUP))
        };
        let group = match (session, own) {
            (Some(session), _) => session_group(SessionId(session)),
            (None, Own::Session(session)) => session_group(session),
            (None, Own::Terminal(terminal)) => data
                .group_of(terminal)
                .map(|group| group.id)
                .ok_or_else(|| refusal(ErrorCode::Unsupported, NO_CALLER_GROUP)),
        };
        Some(group.map(|group| Asked { environment, group }))
    }

    fn worktree_for_bus(&self, asked: Asked, name: &str) -> Result<BusJob, ApiError> {
        let data = self
            .environments
            .data(&asked.environment)
            .ok_or_else(|| refusal(ErrorCode::NotFound, NO_ENVIRONMENT))?;
        let repo = data
            .group(asked.group)
            .and_then(|group| self.git.at(&data.host, &group.root))
            .map(|git| git.repo_root.clone())
            .ok_or_else(|| refusal(ErrorCode::Unsupported, NOT_A_REPO))?;
        let name = sanitize_worktree_name(name)
            .ok_or_else(|| refusal(ErrorCode::BadRequest, BAD_WORKTREE_NAME))?;
        let settings = self.settings.current.worktrees.clone();
        let askpass = if settings.fetch == BaseFetch::FetchFirst {
            self.git_askpass(&asked.environment, &data.host, &repo)
        } else {
            AskpassPlan::Off
        };
        Ok(BusJob::Worktree {
            environment: asked.environment,
            group: asked.group,
            link: self.hosts.link(&data.host),
            repo,
            askpass,
            act: Act::CreateWorktree { name, settings },
        })
    }

    fn changes_for_bus(&self, asked: &Asked) -> Result<BusJob, ApiError> {
        let data = self
            .environments
            .data(&asked.environment)
            .ok_or_else(|| refusal(ErrorCode::NotFound, NO_ENVIRONMENT))?;
        let group = data
            .group(asked.group)
            .ok_or_else(|| refusal(ErrorCode::NotFound, NO_SESSION_GROUP))?;
        let root = self
            .git
            .at(&data.host, &group.root)
            .map_or_else(|| group.root.clone(), |git| git.repo_root.clone());
        Ok(BusJob::Changes {
            link: self.hosts.link(&data.host),
            root,
            group: group.name.clone(),
        })
    }

    fn start_bus_job(&mut self, job: BusJob, reply: futures::channel::oneshot::Sender<Response>) {
        match job {
            BusJob::Worktree {
                environment,
                group,
                link,
                repo,
                askpass,
                act,
            } => {
                let order = ActOrder {
                    asker: Asker::Bus(reply),
                    environment,
                    group: Some(group),
                    link,
                    repo,
                    askpass,
                    act,
                };
                drop(self.start_act(order));
            }
            BusJob::Changes { link, root, group } => {
                let started = self.jobs.run(move || {
                    let response = read_changes(&link, &root, group).map_or_else(
                        |error| Response::error(ErrorCode::Failed, error.to_string()),
                        |changes| Response::Changes(Box::new(changes)),
                    );
                    JobDone::Git(GitJob::Answered(BusAnswer { reply, response }))
                });
                if let Err(error) = started {
                    tracing::warn!(%error, "could not start reading a group's changes");
                }
            }
        }
    }

    pub(super) fn bus_worktree_made(
        &mut self,
        environment: &EnvironmentName,
        from: Option<GroupId>,
        (repo, acted): (PathBuf, Result<Acted, RepoError>),
        reply: futures::channel::oneshot::Sender<Response>,
    ) {
        let worktree = match acted {
            Ok(Acted::Created(worktree)) => worktree,
            Ok(_) => {
                reply_with(reply, Response::error(ErrorCode::Failed, NO_WORKTREE_GROUP));
                return;
            }
            Err(error) => {
                reply_with(reply, Response::error(ErrorCode::Failed, error.to_string()));
                return;
            }
        };
        let branch = worktree.branch.clone();
        let path = worktree.path.clone();
        let Some(opened) = self.open_worktree_group(environment, repo, worktree) else {
            reply_with(reply, Response::error(ErrorCode::Failed, NO_WORKTREE_GROUP));
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
        let group = self
            .environments
            .data(environment)
            .and_then(|data| data.group(opened))
            .map_or_default(|group| group.name.clone());
        reply_with(
            reply,
            Response::WorktreeCreated(Box::new(WorktreeInfo {
                group,
                path,
                branch,
            })),
        );
    }
}

enum BusJob {
    Worktree {
        environment: EnvironmentName,
        group: GroupId,
        link: HostLink,
        repo: PathBuf,
        askpass: AskpassPlan,
        act: Act,
    },
    Changes {
        link: HostLink,
        root: PathBuf,
        group: String,
    },
}
