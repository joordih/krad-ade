use futures::channel::oneshot::Sender as ReplySender;
use gpui_kit::{AppContext as _, Context};
use krad_agents::{ChangeSet, ChangedFile, ErrorCode, Response, WorktreeInfo};
use krad_domain::{EnvironmentName, GroupId, SessionId, WorktreeOrigin};
use krad_repo::{add_worktree, sanitize_worktree_name, status, working_patch, Worktree};
use std::path::PathBuf;

use super::super::super::agent_hooks::{BAD_WORKTREE_NAME, NOT_A_REPO};
use super::super::super::EnvironmentShell;
use super::{
    no_environment, refusal, target_group, NO_SESSION_GROUP, NO_SUCH_GROUP, NO_SUCH_SESSION,
};

impl EnvironmentShell {
    pub(super) fn create_worktree_for(
        &mut self,
        env: EnvironmentName,
        caller: Option<GroupId>,
        name: &str,
        reply: ReplySender<Response>,
        cx: &mut Context<Self>,
    ) {
        let repo_root = self
            .envs
            .get(&env)
            .ok_or_else(no_environment)
            .and_then(|live| {
                let group = target_group(&live.data, caller, None)?;
                live.data
                    .group(group)
                    .ok_or_else(|| refusal(ErrorCode::NotFound, NO_SUCH_GROUP))
            })
            .and_then(|group| {
                self.git_at(&env, &group.root)
                    .map(|git| git.repo_root.clone())
                    .ok_or_else(|| refusal(ErrorCode::Unsupported, NOT_A_REPO))
            });
        let checked = repo_root.and_then(|root| {
            sanitize_worktree_name(name)
                .map(|name| (root, name))
                .ok_or_else(|| refusal(ErrorCode::BadRequest, BAD_WORKTREE_NAME))
        });
        let (repo_root, name) = match checked {
            Ok(checked) => checked,
            Err(error) => {
                reply.send(Response::Error(error)).unwrap_or_default();
                return;
            }
        };
        let link = self.env_link(&env);
        let create = cx.background_spawn({
            let repo_root = repo_root.clone();
            async move { add_worktree(&link, &repo_root, &name) }
        });
        cx.spawn(async move |this, cx| {
            let created = create.await;
            this.update(cx, |shell, cx| {
                let response = match created {
                    Ok(worktree) => shell.open_worktree_group(&env, repo_root, worktree, cx),
                    Err(error) => Response::error(ErrorCode::Failed, error.to_string()),
                };
                reply.send(response).unwrap_or_default();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn open_worktree_group(
        &mut self,
        env: &EnvironmentName,
        repo_root: PathBuf,
        worktree: Worktree,
        cx: &mut Context<Self>,
    ) -> Response {
        let origin = worktree
            .branch
            .clone()
            .map(|branch| WorktreeOrigin { repo_root, branch });
        let selected = self.envs.get(env).and_then(|live| live.selected_group);
        let path = worktree.path.clone();
        let group = self.add_group_to(env, worktree.path, origin, cx);
        let Some(live) = self.envs.get_mut(env) else {
            return Response::Error(no_environment());
        };
        live.selected_group = selected;
        let name = group
            .and_then(|group| live.data.group(group))
            .map_or_default(|group| group.name.clone());
        Response::WorktreeCreated(WorktreeInfo {
            group: name,
            path,
            branch: worktree.branch,
        })
    }

    pub(super) fn send_changes(
        &self,
        env: &EnvironmentName,
        caller: Option<GroupId>,
        session: Option<SessionId>,
        reply: ReplySender<Response>,
        cx: &mut Context<Self>,
    ) {
        let group = self
            .envs
            .get(env)
            .ok_or_else(no_environment)
            .and_then(|live| {
                let group = match session {
                    Some(session) => live
                        .data
                        .session(session)
                        .ok_or_else(|| refusal(ErrorCode::NotFound, NO_SUCH_SESSION))?
                        .group
                        .ok_or_else(|| refusal(ErrorCode::Unsupported, NO_SESSION_GROUP))?,
                    None => target_group(&live.data, caller, None)?,
                };
                live.data
                    .group(group)
                    .cloned()
                    .ok_or_else(|| refusal(ErrorCode::NotFound, NO_SUCH_GROUP))
            });
        let group = match group {
            Ok(group) => group,
            Err(error) => {
                reply.send(Response::Error(error)).unwrap_or_default();
                return;
            }
        };
        let root = self
            .git_at(env, &group.root)
            .map_or_else(|| group.root.clone(), |git| git.repo_root.clone());
        let link = self.env_link(env);
        let reading = cx.background_spawn({
            let (link, root) = (link.clone(), root.clone());
            async move { status(&link, &root) }
        });
        let patching = cx.background_spawn({
            let root = root.clone();
            async move { working_patch(&link, &root) }
        });
        cx.background_spawn(async move {
            let (read, patched) = futures::join!(reading, patching);
            let response = match (read, patched) {
                (Ok(status), Ok(patch)) => Response::Changes(ChangeSet {
                    group: group.name,
                    root,
                    branch: status.branch,
                    files: status
                        .changes
                        .into_iter()
                        .map(|change| ChangedFile {
                            path: change.path,
                            kind: change.kind,
                        })
                        .collect(),
                    patch,
                }),
                (Err(error), _) | (_, Err(error)) => {
                    Response::error(ErrorCode::Failed, error.to_string())
                }
            };
            reply.send(response).unwrap_or_default();
        })
        .detach();
    }
}
