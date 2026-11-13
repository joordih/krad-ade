mod commit_box;
mod menu;
mod requests;
mod switcher;

use gpui_kit::{Context, Window};
use krad_domain::{Branch, CommitMode, ToastKind};
use krad_protocol::{Command, GitAct, GitCommand};
use std::path::PathBuf;

use super::name_dialogs::NameTarget;
use super::EnvironmentShell;
pub(in crate::environment_shell) use commit_box::{ChangeSide, GitUi};
pub(in crate::environment_shell) use requests::{GitFollowUp, GitRequests};
pub(in crate::environment_shell) use switcher::{
    branch_entries, SEARCH_BRANCHES, WORKTREES_HEADER,
};

const NO_GROUP_TEXT: &str = "Select a group first";
const EMPTY_MESSAGE_TEXT: &str = "Type a commit message first";
const FETCHED_TEXT: &str = "Fetched from the remotes";
const PULLED_TEXT: &str = "Pulled the branch";
const PUSHED_TEXT: &str = "Pushed the branch";
const SYNCED_TEXT: &str = "Pulled and pushed the branch";
const TAGS_PUSHED_TEXT: &str = "Pushed the tags";
const STASHED_TEXT: &str = "Set the changes aside";
const POPPED_TEXT: &str = "Brought the stash back";
const COMMITTED_TEXT: &str = "Committed";
const COMMITTED_PUSHED_TEXT: &str = "Committed and pushed";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) enum CommitIntent {
    Commit,
    CommitAndPush,
    Amend,
}

#[derive(Clone, Debug, PartialEq)]
pub(in crate::environment_shell) enum GitAction {
    Fetch,
    Pull,
    Push,
    Sync,
    PushTags,
    Stash,
    PopStash,
    StageAll,
    UnstageAll,
    Commit(CommitIntent),
    CheckoutTo,
    CreateBranch,
    CreateTag,
    DeleteTag(String),
    CheckOut(Branch),
    OpenAsWorktree(Branch),
}

const fn told(text: &'static str) -> GitFollowUp {
    GitFollowUp::Told {
        told: Some(text),
        clears_message: false,
    }
}

const QUIET: GitFollowUp = GitFollowUp::Told {
    told: None,
    clears_message: false,
};

impl EnvironmentShell {
    pub(super) fn run_git_action(
        &mut self,
        action: GitAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (act, follow) = match action {
            GitAction::Fetch => (GitAct::Fetch, told(FETCHED_TEXT)),
            GitAction::Pull => (GitAct::Pull, told(PULLED_TEXT)),
            GitAction::Push => (GitAct::Push, told(PUSHED_TEXT)),
            GitAction::Sync => (GitAct::Sync, told(SYNCED_TEXT)),
            GitAction::PushTags => (GitAct::PushTags, told(TAGS_PUSHED_TEXT)),
            GitAction::Stash => (GitAct::Stash, told(STASHED_TEXT)),
            GitAction::PopStash => (GitAct::PopStash, told(POPPED_TEXT)),
            GitAction::StageAll => (GitAct::StageAll, QUIET),
            GitAction::UnstageAll => (GitAct::UnstageAll, QUIET),
            GitAction::Commit(intent) => {
                self.commit_changes(intent, cx);
                return;
            }
            GitAction::CheckoutTo => {
                self.open_branch_switcher(self.git_ui.menu_origin, cx);
                return;
            }
            GitAction::CreateBranch => {
                self.ask_name(NameTarget::Branch, window, cx);
                return;
            }
            GitAction::CreateTag => {
                self.ask_name(NameTarget::Tag, window, cx);
                return;
            }
            GitAction::DeleteTag(name) => (GitAct::DeleteTag { name }, QUIET),
            GitAction::CheckOut(branch) => (GitAct::CheckOut { branch }, QUIET),
            GitAction::OpenAsWorktree(branch) => (
                GitAct::OpenBranchWorktree { branch },
                GitFollowUp::SelectGroup,
            ),
        };
        self.git_in_group(act, follow, cx);
    }

    pub(super) fn create_named_branch(&mut self, name: String, cx: &mut Context<Self>) {
        self.git_in_group(GitAct::CreateBranch { name }, QUIET, cx);
    }

    pub(super) fn create_named_tag(&mut self, name: String, cx: &mut Context<Self>) {
        self.git_in_group(GitAct::CreateTag { name }, QUIET, cx);
    }

    pub(in crate::environment_shell) fn stage_paths(
        &mut self,
        paths: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        self.git_in_group(GitAct::Stage { paths }, QUIET, cx);
    }

    pub(in crate::environment_shell) fn unstage_paths(
        &mut self,
        paths: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        self.git_in_group(GitAct::Unstage { paths }, QUIET, cx);
    }

    fn commit_changes(&mut self, intent: CommitIntent, cx: &mut Context<Self>) {
        let message = self.git_ui.commit_message(cx);
        if message.trim().is_empty() && intent != CommitIntent::Amend {
            self.toast(ToastKind::Warning, EMPTY_MESSAGE_TEXT, cx);
            return;
        }
        let mode = if intent == CommitIntent::Amend {
            CommitMode::Amend
        } else {
            CommitMode::New
        };
        let push = intent == CommitIntent::CommitAndPush;
        let follow = GitFollowUp::Told {
            told: Some(if push {
                COMMITTED_PUSHED_TEXT
            } else {
                COMMITTED_TEXT
            }),
            clears_message: true,
        };
        self.git_in_group(
            GitAct::Commit {
                message,
                mode,
                push,
            },
            follow,
            cx,
        );
    }

    pub(in crate::environment_shell) fn git_in_group(
        &mut self,
        act: GitAct,
        follow: GitFollowUp,
        cx: &mut Context<Self>,
    ) {
        let Some(group) = self.active_group().map(|group| group.id) else {
            self.toast(ToastKind::Warning, NO_GROUP_TEXT, cx);
            return;
        };
        let command = Command::Git(GitCommand {
            environment: self.active.clone(),
            group,
            act,
        });
        self.post_git(command, follow, cx);
    }
}
