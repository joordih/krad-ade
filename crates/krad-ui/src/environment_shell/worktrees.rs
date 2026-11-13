use gpui_kit::Context;
use krad_domain::{EnvironmentName, GroupId, ToastKind};
use krad_protocol::{Command, GitAct, GitCommand, WorktreeSwept};
use std::path::PathBuf;

use super::git_actions::GitFollowUp;
use super::EnvironmentShell;

const NOT_A_REPO_TEXT: &str = "The selected group is not inside a git repository";
const NOT_A_WORKTREE_TEXT: &str = "The selected group is not a worktree created here";
const REMOVED_PREFIX: &str = "Removed the merged worktree ";

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
        let env = self.active.clone();
        self.create_worktree_then(&env, group, name, GitFollowUp::SelectGroup, cx);
    }

    pub(super) fn create_worktree_then(
        &mut self,
        env: &EnvironmentName,
        group: GroupId,
        name: &str,
        follow: GitFollowUp,
        cx: &mut Context<Self>,
    ) {
        let command = Command::Git(GitCommand {
            environment: env.clone(),
            group,
            act: GitAct::CreateWorktree {
                name: name.to_owned(),
            },
        });
        self.post_git(command, follow, cx);
    }

    pub(super) fn remove_selected_worktree(&mut self, cx: &mut Context<Self>) {
        let Some(group) = self
            .active_group()
            .filter(|group| group.worktree.is_some())
            .map(|group| group.id)
        else {
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
        let command = Command::Git(GitCommand {
            environment: self.active.clone(),
            group,
            act: GitAct::RemoveWorktree,
        });
        let follow = GitFollowUp::Told {
            told: None,
            clears_message: false,
        };
        self.post_git(command, follow, cx);
    }

    pub(super) fn worktree_swept(&mut self, swept: &WorktreeSwept) {
        self.push_toast(
            ToastKind::Success,
            format!("{REMOVED_PREFIX}{}", swept.name),
        );
    }

    pub(super) fn group_repo_root(&self, group: GroupId) -> Option<PathBuf> {
        let live = self.envs.get(&self.active)?;
        let root = &live.data.group(group)?.root;
        self.active_git(root).map(|git| git.repo_root.clone())
    }
}
