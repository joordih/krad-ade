use gpui_kit::{AppContext as _, Context};
use krad_domain::{GroupId, WorktreeOrigin};
use krad_repo::{add_worktree, remove_worktree, sanitize_worktree_name};

use super::EnvironmentShell;

const NOT_A_REPO_TEXT: &str = "The selected group is not inside a git repository";
const NOT_A_WORKTREE_TEXT: &str = "The selected group is not a worktree created here";
const INVALID_NAME_TEXT: &str = "Use letters, digits, '-', '_' or '.' for the worktree name";

impl EnvironmentShell {
    pub(super) fn start_new_worktree(&mut self, cx: &mut Context<Self>) {
        let Some(group) = self.active_group().map(|group| group.id) else {
            self.toast(NOT_A_REPO_TEXT, cx);
            return;
        };
        if self.group_repo_root(group).is_none() {
            self.toast(NOT_A_REPO_TEXT, cx);
            return;
        }
        self.start_worktree_draft(group, cx);
    }

    pub(super) fn create_worktree(&mut self, group: GroupId, name: &str, cx: &mut Context<Self>) {
        let Some(repo_root) = self.group_repo_root(group) else {
            self.toast(NOT_A_REPO_TEXT, cx);
            return;
        };
        let Some(name) = sanitize_worktree_name(name) else {
            self.toast(INVALID_NAME_TEXT, cx);
            return;
        };
        let create = cx.background_spawn({
            let repo_root = repo_root.clone();
            async move { add_worktree(&repo_root, &name) }
        });
        cx.spawn(async move |this, cx| {
            let created = create.await;
            this.update(cx, |shell, cx| match created {
                Ok(worktree) => {
                    let origin = worktree
                        .branch
                        .map(|branch| WorktreeOrigin { repo_root, branch });
                    shell.add_group(worktree.path, origin, cx);
                }
                Err(error) => shell.toast(&error.to_string(), cx),
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
            self.toast(NOT_A_WORKTREE_TEXT, cx);
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
        let remove = cx.background_spawn(async move { remove_worktree(&origin.repo_root, &root) });
        cx.spawn(async move |this, cx| {
            let removed = remove.await;
            this.update(cx, |shell, cx| match removed {
                Ok(()) => {
                    shell.select_group(group, cx);
                    shell.remove_selected_group(cx);
                }
                Err(error) => shell.toast(&error.to_string(), cx),
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn group_repo_root(&self, group: GroupId) -> Option<std::path::PathBuf> {
        let live = self.envs.get(&self.active)?;
        let root = &live.data.group(group)?.root;
        self.git.get(root).map(|git| git.repo_root.clone())
    }

    pub(super) fn toast(&mut self, text: &str, cx: &mut Context<Self>) {
        self.push_toast(text.to_owned());
        cx.notify();
    }
}
