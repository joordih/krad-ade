mod sweep;

use gpui_kit::{AppContext as _, Context};
use krad_domain::{
    AppEvent, BaseFetch, EnvironmentName, GroupId, ToastKind, WorktreeOrigin, WorktreeSettings,
};
use krad_host::HostLink;
use krad_repo::{
    add_planned_worktree, remove_worktree, sanitize_worktree_name, Askpass, RepoError, Worktree,
};
use std::path::Path;

use super::agent_bus::AskpassPlan;
use super::EnvironmentShell;
pub(super) use sweep::WorktreeSweep;

const NOT_A_REPO_TEXT: &str = "The selected group is not inside a git repository";
const NOT_A_WORKTREE_TEXT: &str = "The selected group is not a worktree created here";
const INVALID_NAME_TEXT: &str = "Use letters, digits, '-', '_' or '.' for the worktree name";

pub(super) struct WorktreeRecipe {
    settings: WorktreeSettings,
    askpass: AskpassPlan,
}

impl WorktreeRecipe {
    #[tracing::instrument(skip_all)]
    pub(super) fn add(
        self,
        link: &HostLink,
        repo_root: &Path,
        name: &str,
    ) -> Result<Worktree, RepoError> {
        let askpass = match self.settings.fetch {
            BaseFetch::FetchFirst => self.askpass.resolve(),
            BaseFetch::Off => Askpass::default(),
        };
        let planned = add_planned_worktree(link, repo_root, name, &self.settings, &askpass)?;
        for setback in &planned.setbacks {
            tracing::warn!(%setback, "the new worktree skipped a step");
        }
        Ok(planned.worktree)
    }
}

impl EnvironmentShell {
    pub(super) fn start_new_worktree(&mut self, cx: &mut Context<Self>) {
        let Some(group) = self.active_group().map(|group| group.id) else {
            self.toast(ToastKind::Warning, NOT_A_REPO_TEXT, cx);
            return;
        };
        if self.group_repo_root(group).is_none() {
            self.toast(ToastKind::Warning, NOT_A_REPO_TEXT, cx);
            return;
        }
        self.start_worktree_draft(group, cx);
    }

    pub(super) fn create_worktree(&mut self, group: GroupId, name: &str, cx: &mut Context<Self>) {
        let Some(repo_root) = self.group_repo_root(group) else {
            self.toast(ToastKind::Warning, NOT_A_REPO_TEXT, cx);
            return;
        };
        let Some(name) = sanitize_worktree_name(name) else {
            self.toast(ToastKind::Warning, INVALID_NAME_TEXT, cx);
            return;
        };
        let env = self.active.clone();
        let link = self.active_link();
        let recipe = self.worktree_recipe(&env, &repo_root);
        let create = cx.background_spawn({
            let repo_root = repo_root.clone();
            async move { recipe.add(&link, &repo_root, &name) }
        });
        cx.spawn(async move |this, cx| {
            let created = create.await;
            this.update(cx, |shell, cx| match created {
                Ok(worktree) => {
                    let origin = worktree
                        .branch
                        .map(|branch| WorktreeOrigin { repo_root, branch });
                    let created = shell.add_group_to(&env, worktree.path, origin, cx);
                    if let Some(created) = created {
                        let event = AppEvent::WorktreeCreated { from: group };
                        shell.publish_event(&env, Some(created), event);
                    }
                }
                Err(error) => shell.toast(ToastKind::Error, &error.to_string(), cx),
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn remove_selected_worktree(&mut self, cx: &mut Context<Self>) {
        let Some((group, root, origin)) = self.active_group().and_then(|group| {
            group
                .worktree
                .clone()
                .map(|origin| (group.id, group.root.clone(), origin))
        }) else {
            self.toast(ToastKind::Warning, NOT_A_WORKTREE_TEXT, cx);
            return;
        };
        let terminals: Vec<_> = self.envs.get(&self.active).map_or_default(|live| {
            live.data
                .nodes
                .iter()
                .filter(|node| node.group == Some(group))
                .map(|node| node.id)
                .collect()
        });
        for id in terminals {
            self.close_terminal(id, cx);
        }
        let env = self.active.clone();
        let link = self.active_link();
        let remove =
            cx.background_spawn(async move { remove_worktree(&link, &origin.repo_root, &root) });
        cx.spawn(async move |this, cx| {
            let removed = remove.await;
            this.update(cx, |shell, cx| match removed {
                Ok(()) => {
                    shell.publish_event(&env, Some(group), AppEvent::WorktreeRemoved);
                    shell.select_group(group, cx);
                    shell.remove_selected_group(cx);
                }
                Err(error) => shell.toast(ToastKind::Error, &error.to_string(), cx),
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn worktree_recipe(
        &self,
        env: &EnvironmentName,
        repo_root: &Path,
    ) -> WorktreeRecipe {
        let settings = self.settings.worktrees.clone();
        let askpass = match settings.fetch {
            BaseFetch::FetchFirst => self.git_askpass(env, repo_root),
            BaseFetch::Off => AskpassPlan::Off,
        };
        WorktreeRecipe { settings, askpass }
    }

    pub(super) fn group_repo_root(&self, group: GroupId) -> Option<std::path::PathBuf> {
        let live = self.envs.get(&self.active)?;
        let root = &live.data.group(group)?.root;
        self.active_git(root).map(|git| git.repo_root.clone())
    }
}
