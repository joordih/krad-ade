mod owner;

use gpui_kit::{
    div, px, rgb, AnyElement, AppContext as _, Context, ElementId, IntoElement, ParentElement,
    Styled,
};
use krad_domain::review::{
    ci_fix_prompt, ChangeStat, CiFailure, CiStatus, HeadRepository, Job, JobId, ProjectPath,
};
use krad_domain::{EnvironmentName, GroupId, NodeKind, ToastKind, WorktreeOrigin};
use krad_repo::add_branch_worktree;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::detail::RequestDetail;
use super::{ci_glyph, Remote};
use crate::components::{Button, ButtonVariant};
use crate::environment_shell::EnvironmentShell;
use crate::theme::{palette, ICON_SM, SPACE_2, TEXT_XS};
use owner::{branch_owner, pick_branch, shares_repository, ChatSeat, FixOwner};

const BTN_FIX_JOB: &str = "review-fix-job";
const FIX_LABEL: &str = "Fix with agent";
const FIX_TOOLTIP: &str = "Send the failing step's log to the agent working on this branch";
const READING_LOG_LABEL: &str = "Reading the log…";
const FIX_ROW_H: f32 = 28.0;
const SENT_PREFIX: &str = "Sent the failure to the agent on ";
const STARTED_PREFIX: &str = "Started an agent on ";
const NO_REPO_TEXT: &str = "The group is not a git repository.";
const NO_BRANCH_PREFIX: &str = "Fetch the branch first: no local or remote branch is named ";
const NO_CHAT_TEXT: &str = "Could not open a chat for the fix.";
const FORK_PREFIX: &str =
    "The branch lives in a fork, not in this group's repository; check it out from ";
const UNNAMED_FORK: &str = "the fork";
const MOVED_ON_TEXT: &str =
    "The environment changed while the log was read; open the review again to send the fix.";

struct FixOrder {
    env: EnvironmentName,
    root: PathBuf,
    project: ProjectPath,
    head: HeadRepository,
    branch: String,
    prompt: String,
}

impl EnvironmentShell {
    pub(super) fn failed_job_rows(
        detail: &RequestDetail,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Remote::Ready(jobs) = &detail.jobs else {
            return Vec::new();
        };
        let pal = palette(cx);
        failed_jobs(jobs)
            .map(|job| {
                let action = if detail.fixing.contains(&job.id) {
                    div()
                        .text_color(rgb(pal.text_muted))
                        .child(READING_LOG_LABEL)
                        .into_any_element()
                } else {
                    let id = job.id;
                    Button::new(ElementId::from((BTN_FIX_JOB, id.0)))
                        .label(FIX_LABEL)
                        .tooltip(FIX_TOOLTIP)
                        .small()
                        .variant(ButtonVariant::Secondary)
                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                            this.fix_job_with_agent(id, cx);
                        }))
                        .into_any_element()
                };
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(SPACE_2))
                    .h(px(FIX_ROW_H))
                    .text_size(px(TEXT_XS))
                    .text_color(rgb(pal.text_strong))
                    .child(ci_glyph(Some(job.status), ICON_SM, pal))
                    .child(div().flex_1().min_w_0().truncate().child(job.name.clone()))
                    .child(action)
                    .into_any_element()
            })
            .collect()
    }

    fn fix_job_with_agent(&mut self, job: JobId, cx: &mut Context<Self>) {
        let Some(client) = self.host_client() else {
            return;
        };
        let env = self.active.clone();
        let Some(detail) = self.review.detail.as_mut() else {
            return;
        };
        let Remote::Ready(jobs) = &detail.jobs else {
            return;
        };
        let Some(failed) = jobs.iter().find(|candidate| candidate.id == job).cloned() else {
            return;
        };
        if !detail.fixing.insert(job) {
            return;
        }
        let project = detail.project.clone();
        let request = detail.request.clone();
        let log_project = project.clone();
        let root = detail.root.clone();
        let changes = change_stats(detail);
        let host = client.code_host();
        let read = cx.background_spawn({
            let client = Arc::clone(&client);
            async move { client.job_log(&log_project, job) }
        });
        cx.notify();
        cx.spawn(async move |this, cx| {
            let log = read.await;
            this.update(cx, |shell, cx| {
                if let Some(detail) = shell.review.detail.as_mut() {
                    detail.fixing.remove(&job);
                }
                cx.notify();
                let log = match log {
                    Ok(log) => log,
                    Err(error) => {
                        shell.toast(ToastKind::Error, &error.to_string(), cx);
                        return;
                    }
                };
                let prompt = ci_fix_prompt(&CiFailure {
                    host,
                    request: &request,
                    job: &failed,
                    log: &log,
                    changes: &changes,
                });
                let order = FixOrder {
                    env,
                    root,
                    project,
                    head: request.head_repository,
                    branch: request.source_branch,
                    prompt,
                };
                shell.deliver_ci_fix(order, cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn deliver_ci_fix(&mut self, order: FixOrder, cx: &mut Context<Self>) {
        if order.env != self.active {
            self.toast(ToastKind::Info, MOVED_ON_TEXT, cx);
            return;
        }
        if !order.head.lives_in(&order.project) {
            let fork = match &order.head {
                HeadRepository::Fork(Some(fork)) => fork.to_string(),
                HeadRepository::Fork(None) | HeadRepository::Same => UNNAMED_FORK.to_owned(),
            };
            self.toast(ToastKind::Info, &format!("{FORK_PREFIX}{fork}"), cx);
            return;
        }
        match self.ci_fix_owner(&order.env, &order.root, &order.branch) {
            FixOwner::Chat(id) => {
                if self.queue_chat_prompt(&order.env, id, &order.prompt, cx) {
                    self.toast(
                        ToastKind::Info,
                        &format!("{SENT_PREFIX}{}", order.branch),
                        cx,
                    );
                    return;
                }
                let group = self
                    .envs
                    .get(&order.env)
                    .and_then(|live| live.data.group_of(id))
                    .map(|group| group.id);
                self.start_fix_chat(group, &order, cx);
            }
            FixOwner::Group(group) => self.start_fix_chat(Some(group), &order, cx),
            FixOwner::Nobody => self.open_fix_worktree(order, cx),
        }
    }

    fn ci_fix_owner(&self, env: &EnvironmentName, root: &Path, branch: &str) -> FixOwner {
        let Some(live) = self.envs.get(env) else {
            return FixOwner::Nobody;
        };
        let Some(repository) = self.git_at(env, root) else {
            return FixOwner::Nobody;
        };
        let in_repository = live
            .data
            .groups
            .iter()
            .filter(|group| shares_repository(repository, &group.root));
        let groups = in_repository.map(|group| {
            let checked_out = self
                .git_at(env, &group.root)
                .and_then(|git| git.status.branch.as_deref())
                .or_else(|| group.worktree.as_ref().map(|origin| origin.branch.as_str()));
            (group.id, checked_out)
        });
        let chats = live
            .data
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Chat)
            .map(|node| ChatSeat {
                id: node.id,
                group: node.group,
                worker: live.data.worker(node.id).is_some(),
            });
        branch_owner(groups, chats, branch)
    }

    fn start_fix_chat(&mut self, group: Option<GroupId>, order: &FixOrder, cx: &mut Context<Self>) {
        let Some(id) = self.start_chat_session(group, cx) else {
            self.toast(ToastKind::Warning, NO_CHAT_TEXT, cx);
            return;
        };
        if self.queue_chat_prompt(&order.env, id, &order.prompt, cx) {
            self.toast(
                ToastKind::Success,
                &format!("{STARTED_PREFIX}{}", order.branch),
                cx,
            );
        } else {
            self.toast(ToastKind::Warning, NO_CHAT_TEXT, cx);
        }
    }

    fn open_fix_worktree(&mut self, order: FixOrder, cx: &mut Context<Self>) {
        let Some(git) = self.git_at(&order.env, &order.root) else {
            self.toast(ToastKind::Warning, NO_REPO_TEXT, cx);
            return;
        };
        let Some(branch) = pick_branch(&git.refs.branches, &order.branch).cloned() else {
            self.toast(
                ToastKind::Warning,
                &format!("{NO_BRANCH_PREFIX}{}", order.branch),
                cx,
            );
            return;
        };
        let repo_root = git.repo_root.clone();
        let link = self.env_link(&order.env);
        let create = cx.background_spawn({
            let repo_root = repo_root.clone();
            async move { add_branch_worktree(&link, &repo_root, &branch) }
        });
        cx.spawn(async move |this, cx| {
            let created = create.await;
            this.update(cx, |shell, cx| {
                let worktree = match created {
                    Ok(worktree) => worktree,
                    Err(error) => {
                        shell.toast(ToastKind::Error, &error.to_string(), cx);
                        return;
                    }
                };
                let origin = worktree
                    .branch
                    .map(|branch| WorktreeOrigin { repo_root, branch });
                let group = shell.add_group_to(&order.env, worktree.path, origin, cx);
                shell.start_fix_chat(group, &order, cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }
}

fn change_stats(detail: &RequestDetail) -> Vec<ChangeStat> {
    let Remote::Ready(files) = &detail.files else {
        return Vec::new();
    };
    files
        .iter()
        .map(|file| ChangeStat {
            path: file.change.new_path.clone(),
            added: file.added,
            removed: file.removed,
        })
        .collect()
}

fn failed_jobs(jobs: &[Job]) -> impl Iterator<Item = &Job> {
    jobs.iter().filter(|job| job.status == CiStatus::Failed)
}
