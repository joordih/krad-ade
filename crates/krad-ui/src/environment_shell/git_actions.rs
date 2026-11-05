mod commit_box;
mod menu;
mod switcher;

use gpui_kit::{AppContext as _, Context, Window};
use krad_domain::WorktreeOrigin;
use krad_host::HostLink;
use krad_repo::{
    add_branch_worktree, check_out, commit, create_branch, create_tag, delete_tag, fetch,
    pop_stash, pull, push, push_tags, stage, stage_all, stash_changes, sync, unstage, unstage_all,
    Askpass, Branch, BranchPlace, CommitMode, CommitScope, PushTarget, Refs, RepoError,
    ORIGIN_REMOTE,
};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::name_dialogs::NameTarget;
use super::EnvironmentShell;
pub(in crate::environment_shell) use commit_box::{ChangeSide, GitUi};
pub(in crate::environment_shell) use switcher::{
    branch_entries, SEARCH_BRANCHES, WORKTREES_HEADER,
};

const NO_REPO_TEXT: &str = "The selected group is not inside a git repository";
const DETACHED_PUSH_TEXT: &str = "Check out a branch before pushing";
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

#[derive(Clone, Copy)]
struct JobEnd {
    told: Option<&'static str>,
    clears_message: bool,
}

impl JobEnd {
    const QUIET: Self = Self {
        told: None,
        clears_message: false,
    };

    const fn told(text: &'static str) -> Self {
        Self {
            told: Some(text),
            clears_message: false,
        }
    }
}

fn preferred_remote(refs: &Refs) -> String {
    let remotes: BTreeSet<&str> = refs
        .remote()
        .filter_map(|branch| match &branch.place {
            BranchPlace::Remote(remote) => Some(remote.as_str()),
            BranchPlace::Local => None,
        })
        .collect();
    if remotes.is_empty() || remotes.contains(ORIGIN_REMOTE) {
        return ORIGIN_REMOTE.to_owned();
    }
    remotes
        .into_iter()
        .next()
        .map_or_else(|| ORIGIN_REMOTE.to_owned(), str::to_owned)
}

impl EnvironmentShell {
    pub(super) fn run_git_action(
        &mut self,
        action: GitAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            GitAction::Fetch => {
                self.git_job(JobEnd::told(FETCHED_TEXT), fetch, cx);
            }
            GitAction::Pull => {
                self.git_job(JobEnd::told(PULLED_TEXT), pull, cx);
            }
            GitAction::Push => {
                if let Some(target) = self.push_target(cx) {
                    self.git_job(
                        JobEnd::told(PUSHED_TEXT),
                        move |link, repo, ask| push(link, repo, &target, ask),
                        cx,
                    );
                }
            }
            GitAction::Sync => {
                if let Some(target) = self.push_target(cx) {
                    self.git_job(
                        JobEnd::told(SYNCED_TEXT),
                        move |link, repo, ask| sync(link, repo, &target, ask),
                        cx,
                    );
                }
            }
            GitAction::PushTags => self.git_job(JobEnd::told(TAGS_PUSHED_TEXT), push_tags, cx),
            GitAction::Stash => self.git_job(
                JobEnd::told(STASHED_TEXT),
                |link, repo, _| stash_changes(link, repo),
                cx,
            ),
            GitAction::PopStash => {
                self.git_job(
                    JobEnd::told(POPPED_TEXT),
                    |link, repo, _| pop_stash(link, repo),
                    cx,
                );
            }
            GitAction::StageAll => {
                self.git_job(JobEnd::QUIET, |link, repo, _| stage_all(link, repo), cx);
            }
            GitAction::UnstageAll => {
                self.git_job(JobEnd::QUIET, |link, repo, _| unstage_all(link, repo), cx);
            }
            GitAction::Commit(intent) => self.commit_changes(intent, cx),
            GitAction::CheckoutTo => self.open_branch_switcher(self.git_ui.menu_origin, cx),
            GitAction::CreateBranch => self.ask_name(NameTarget::Branch, window, cx),
            GitAction::CreateTag => self.ask_name(NameTarget::Tag, window, cx),
            GitAction::DeleteTag(name) => self.git_job(
                JobEnd::QUIET,
                move |link, repo, _| delete_tag(link, repo, &name),
                cx,
            ),
            GitAction::CheckOut(branch) => self.git_job(
                JobEnd::QUIET,
                move |link, repo, _| check_out(link, repo, &branch),
                cx,
            ),
            GitAction::OpenAsWorktree(branch) => self.open_branch_worktree(branch, cx),
        }
    }

    pub(super) fn create_named_branch(&mut self, name: String, cx: &mut Context<Self>) {
        self.git_job(
            JobEnd::QUIET,
            move |link, repo, _| create_branch(link, repo, &name),
            cx,
        );
    }

    pub(super) fn create_named_tag(&mut self, name: String, cx: &mut Context<Self>) {
        self.git_job(
            JobEnd::QUIET,
            move |link, repo, _| create_tag(link, repo, &name),
            cx,
        );
    }

    pub(in crate::environment_shell) fn stage_paths(
        &mut self,
        paths: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        self.git_job(
            JobEnd::QUIET,
            move |link, repo, _| stage(link, repo, &paths),
            cx,
        );
    }

    pub(in crate::environment_shell) fn unstage_paths(
        &mut self,
        paths: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        self.git_job(
            JobEnd::QUIET,
            move |link, repo, _| unstage(link, repo, &paths),
            cx,
        );
    }

    fn commit_changes(&mut self, intent: CommitIntent, cx: &mut Context<Self>) {
        let message = self.git_ui.commit_message(cx);
        if message.trim().is_empty() && intent != CommitIntent::Amend {
            self.toast(EMPTY_MESSAGE_TEXT, cx);
            return;
        }
        let Some(git) = self.group_git() else {
            self.toast(NO_REPO_TEXT, cx);
            return;
        };
        let scope = if git.status.staged.is_empty() {
            CommitScope::Everything
        } else {
            CommitScope::Staged
        };
        let mode = if intent == CommitIntent::Amend {
            CommitMode::Amend
        } else {
            CommitMode::New
        };
        let pushed = match intent {
            CommitIntent::CommitAndPush => match self.push_target(cx) {
                Some(target) => Some(target),
                None => return,
            },
            CommitIntent::Commit | CommitIntent::Amend => None,
        };
        let end = JobEnd {
            told: Some(if pushed.is_some() {
                COMMITTED_PUSHED_TEXT
            } else {
                COMMITTED_TEXT
            }),
            clears_message: true,
        };
        self.git_job(
            end,
            move |link, repo, ask| {
                commit(link, repo, &message, mode, scope)?;
                pushed.map_or(Ok(()), |target| push(link, repo, &target, ask))
            },
            cx,
        );
    }

    fn push_target(&mut self, cx: &mut Context<Self>) -> Option<PushTarget> {
        let Some(git) = self.group_git() else {
            self.toast(NO_REPO_TEXT, cx);
            return None;
        };
        if git.status.upstream.is_some() {
            return Some(PushTarget::Upstream);
        }
        let Some(branch) = git.status.branch.clone() else {
            self.toast(DETACHED_PUSH_TEXT, cx);
            return None;
        };
        Some(PushTarget::Publish {
            remote: preferred_remote(&git.refs),
            branch,
        })
    }

    fn open_branch_worktree(&mut self, branch: Branch, cx: &mut Context<Self>) {
        let Some(repo_root) = self.group_git().map(|git| git.repo_root.clone()) else {
            self.toast(NO_REPO_TEXT, cx);
            return;
        };
        let link = self.active_link();
        let create = cx.background_spawn({
            let repo_root = repo_root.clone();
            async move { add_branch_worktree(&link, &repo_root, &branch) }
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

    fn git_job<F>(&mut self, end: JobEnd, work: F, cx: &mut Context<Self>)
    where
        F: FnOnce(&HostLink, &Path, &Askpass) -> Result<(), RepoError> + Send + 'static,
    {
        let Some(repo) = self.group_git().map(|git| git.repo_root.clone()) else {
            self.toast(NO_REPO_TEXT, cx);
            return;
        };
        let env = self.active.clone();
        let link = self.active_link();
        let plan = self.git_askpass(&env, &repo);
        let job = cx.background_spawn(async move { work(&link, &repo, &plan.resolve()) });
        cx.spawn(async move |this, cx| {
            let done = job.await;
            this.update(cx, |shell, cx| {
                match done {
                    Ok(()) => {
                        if end.clears_message {
                            shell.git_ui.clear_message();
                        }
                        if let Some(told) = end.told {
                            shell.toast(told, cx);
                        }
                    }
                    Err(error) => shell.toast(&error.to_string(), cx),
                }
                shell.refresh_git(cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remote_branch(remote: &str, name: &str) -> Branch {
        Branch {
            name: name.to_owned(),
            place: BranchPlace::Remote(remote.to_owned()),
            upstream: None,
            is_head: false,
        }
    }

    #[test]
    fn a_new_branch_is_published_to_origin_or_else_the_first_remote() {
        assert_eq!(preferred_remote(&Refs::default()), ORIGIN_REMOTE);
        let both = Refs {
            branches: vec![
                remote_branch("fork", "main"),
                remote_branch(ORIGIN_REMOTE, "main"),
            ],
            tags: Vec::new(),
        };
        assert_eq!(preferred_remote(&both), ORIGIN_REMOTE);
        let fork = Refs {
            branches: vec![
                remote_branch("upstream", "main"),
                remote_branch("fork", "x"),
            ],
            tags: Vec::new(),
        };
        assert_eq!(preferred_remote(&fork), "fork");
    }
}
