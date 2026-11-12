use gpui_kit::{AppContext as _, Context, PromptButton, PromptLevel, Window};
use krad_chat::{new_session_id, LiveSession};
use krad_domain::{AgentPreset, CloudRunnerId, EnvironmentName, GroupId, SessionSpec, ToastKind};
use krad_repo::{resolved_remote_url, working_patch_with_untracked, ORIGIN_REMOTE};
use std::path::PathBuf;

use super::super::native_alert::NATIVE_ALERTS;
use super::super::terminal_card::home_dir;
use super::super::terminal_lifecycle::NodeGroup;
use super::super::EnvironmentShell;
use super::carry::{carry_chats, CarriedChat, CarriedStart};
use super::naming::{
    cloud_chat_harness, cloud_session_name, holds_cloud_work, new_nonce, offered_patch,
    reached_limit, session_slug, upstream_branch, upstream_remote,
};
use super::workspace::{prepare_workspace, PreparedWorkspace, WorkspaceError, WorkspaceRequest};

const NO_REMOTE_TOAST: &str = "The group has no git remote to clone on the runner";
const AT_LIMIT_TOAST: &str =
    "You already run as many cloud sessions as Settings allows; close one first";
const PREPARING_PREFIX: &str = "Preparing the cloud session on";
const REMOTE_UNREAD: &str = "Could not read the group's git remote";
const RUNNER_FAILED_PREFIX: &str = "The cloud session could not start on";
pub(super) const PATCH_TITLE: &str = "Bring your uncommitted changes?";
pub(super) const PATCH_DETAIL: &str = "The cloud session starts from the group's branch on its remote. Your uncommitted changes, new files included, can be applied there as a patch.";
pub(super) const PATCH_CONFIRM: &str = "Bring them";
pub(super) const PATCH_LEAVE: &str = "Leave them";
pub(super) const PATCH_CANCEL: &str = "Cancel";
const BRING_ANSWER: usize = 0;
const LEAVE_ANSWER: usize = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PatchAnswer {
    Bring,
    Leave,
    Cancel,
}

#[derive(Clone, Debug, Default)]
pub(in crate::environment_shell) struct CloudOrder {
    pub(in crate::environment_shell) name: Option<String>,
    pub(in crate::environment_shell) carried: Vec<CarriedChat>,
}

#[derive(Clone, Debug)]
pub(in crate::environment_shell) struct CloudDraft {
    env: EnvironmentName,
    group: GroupId,
    name: String,
    request: WorkspaceRequest,
    carried: Vec<CarriedChat>,
}

#[derive(Clone, Debug)]
pub(in crate::environment_shell) struct PendingCloudSession(CloudDraft);

#[derive(Clone, Debug)]
pub(in crate::environment_shell) struct PatchQuestion {
    pub(super) draft: CloudDraft,
    pub(super) patch: String,
}

struct GroupFacts {
    group: GroupId,
    runner: CloudRunnerId,
    env: EnvironmentName,
    root: PathBuf,
    name: String,
    upstream: Option<String>,
    order: CloudOrder,
}

type RemoteRead = (Result<Option<String>, krad_repo::RepoError>, String);
type Prepared = Result<(PreparedWorkspace, Vec<CarriedStart>), WorkspaceError>;

impl EnvironmentShell {
    fn running_cloud_sessions(&self) -> usize {
        self.envs
            .values()
            .flat_map(|live| {
                live.data
                    .sessions
                    .iter()
                    .filter(|session| holds_cloud_work(session, &live.data.nodes))
            })
            .count()
    }

    fn group_facts(
        &self,
        group: GroupId,
        runner: CloudRunnerId,
        order: CloudOrder,
    ) -> Option<GroupFacts> {
        let env = self.active.clone();
        let found = self.envs.get(&env)?.data.group(group)?;
        let upstream = self
            .git_at(&env, &found.root)
            .and_then(|git| git.status.upstream.clone());
        Some(GroupFacts {
            group,
            runner,
            root: found.root.clone(),
            name: found.name.clone(),
            upstream,
            env,
            order,
        })
    }

    pub(in crate::environment_shell) fn new_cloud_session(
        &mut self,
        group: GroupId,
        runner: CloudRunnerId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.create_cloud_session(group, runner, CloudOrder::default(), window, cx);
    }

    pub(in crate::environment_shell) fn create_cloud_session(
        &mut self,
        group: GroupId,
        runner: CloudRunnerId,
        order: CloudOrder,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if reached_limit(
            self.running_cloud_sessions(),
            self.settings.cloud.max_sessions,
        ) {
            self.toast(ToastKind::Warning, AT_LIMIT_TOAST, cx);
            return;
        }
        let Some(facts) = self.group_facts(group, runner, order) else {
            return;
        };
        let link = self.env_link(&facts.env);
        let remote = upstream_remote(facts.upstream.as_deref())
            .unwrap_or(ORIGIN_REMOTE)
            .to_owned();
        let root = facts.root.clone();
        let read = cx.background_spawn(async move {
            let home = home_dir().map(PathBuf::from);
            let url = resolved_remote_url(&link, &root, &remote, home.as_deref());
            let diff = working_patch_with_untracked(&link, &root).unwrap_or_else(|error| {
                tracing::debug!(%error, "no uncommitted changes to offer");
                String::new()
            });
            (url, diff)
        });
        cx.spawn_in(window, async move |this, cx| {
            let read = read.await;
            this.update_in(cx, |shell, window, cx| {
                shell.remote_read(facts, read, window, cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn remote_read(
        &mut self,
        facts: GroupFacts,
        (url, diff): RemoteRead,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let GroupFacts {
            group,
            runner,
            order,
            ..
        } = facts;
        let remote_url = match url {
            Ok(Some(url)) => url,
            Ok(None) => return self.toast(ToastKind::Warning, NO_REMOTE_TOAST, cx),
            Err(error) => {
                return self.toast(ToastKind::Error, &format!("{REMOTE_UNREAD}: {error}"), cx);
            }
        };
        let ordinal = self.envs.get(&facts.env).map_or(0, |live| {
            live.data
                .sessions_of(Some(group))
                .filter(|session| session.cloud.is_some())
                .count()
        }) + 1;
        let draft = CloudDraft {
            name: order
                .name
                .unwrap_or_else(|| cloud_session_name(&facts.name, ordinal)),
            request: WorkspaceRequest {
                runner,
                remote_url,
                upstream_branch: upstream_branch(facts.upstream.as_deref()).map(str::to_owned),
                session_slug: session_slug(&facts.name, &new_nonce()),
                patch: None,
            },
            env: facts.env,
            group,
            carried: order.carried,
        };
        match offered_patch(diff) {
            Some(patch) => self.ask_for_patch(PatchQuestion { draft, patch }, window, cx),
            None => self.queue_cloud_session(draft, cx),
        }
    }

    fn ask_for_patch(
        &mut self,
        question: PatchQuestion,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !NATIVE_ALERTS {
            self.cloud.asking = Some(question);
            cx.notify();
            return;
        }
        let answer = window.prompt(
            PromptLevel::Warning,
            PATCH_TITLE,
            Some(PATCH_DETAIL),
            &[
                PromptButton::ok(PATCH_CONFIRM),
                PromptButton::new(PATCH_LEAVE),
                PromptButton::cancel(PATCH_CANCEL),
            ],
            cx,
        );
        cx.spawn(async move |this, cx| {
            let answer = match answer.await {
                Ok(BRING_ANSWER) => PatchAnswer::Bring,
                Ok(LEAVE_ANSWER) => PatchAnswer::Leave,
                _ => PatchAnswer::Cancel,
            };
            this.update(cx, |shell, cx| shell.answer_patch(question, answer, cx))
                .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn answer_patch(
        &mut self,
        question: PatchQuestion,
        answer: PatchAnswer,
        cx: &mut Context<Self>,
    ) {
        let PatchQuestion { mut draft, patch } = question;
        match answer {
            PatchAnswer::Cancel => return,
            PatchAnswer::Bring => draft.request.patch = Some(patch),
            PatchAnswer::Leave => {}
        }
        self.queue_cloud_session(draft, cx);
    }

    fn queue_cloud_session(&mut self, draft: CloudDraft, cx: &mut Context<Self>) {
        let runner = draft.request.runner;
        let text = format!("{PREPARING_PREFIX} {}…", self.runner_name(runner));
        self.toast(ToastKind::Info, &text, cx);
        self.cloud.pending.push(PendingCloudSession(draft));
        self.ensure_runner_ready(runner, cx);
        if self.runner_link(runner).is_some() {
            self.start_pending_cloud_sessions(runner, cx);
        }
    }

    pub(super) fn start_pending_cloud_sessions(
        &mut self,
        runner: CloudRunnerId,
        cx: &mut Context<Self>,
    ) {
        let Some(link) = self.runner_link(runner) else {
            return;
        };
        let (starting, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.cloud.pending)
            .into_iter()
            .partition(|PendingCloudSession(draft)| draft.request.runner == runner);
        self.cloud.pending = waiting;
        for PendingCloudSession(mut draft) in starting {
            let link = link.clone();
            let request = draft.request.clone();
            let carried = std::mem::take(&mut draft.carried);
            let prepare = cx.background_spawn(async move {
                let prepared = prepare_workspace(&link, &request)?;
                let starts = carry_chats(&link, &prepared.placement.worktree, carried);
                Ok((prepared, starts))
            });
            cx.spawn(async move |this, cx| {
                let prepared = prepare.await;
                this.update(cx, |shell, cx| {
                    shell.workspace_prepared(draft, prepared, cx);
                })
                .unwrap_or_default();
            })
            .detach();
        }
    }

    pub(super) fn fail_pending_cloud_sessions(
        &mut self,
        runner: CloudRunnerId,
        cx: &mut Context<Self>,
    ) {
        let before = self.cloud.pending.len();
        self.cloud
            .pending
            .retain(|PendingCloudSession(draft)| draft.request.runner != runner);
        if self.cloud.pending.len() == before {
            return;
        }
        let text = format!(
            "{RUNNER_FAILED_PREFIX} {}: {}",
            self.runner_name(runner),
            self.runner_state(runner).label()
        );
        self.toast(ToastKind::Error, &text, cx);
    }

    fn workspace_prepared(
        &mut self,
        draft: CloudDraft,
        prepared: Prepared,
        cx: &mut Context<Self>,
    ) {
        let (prepared, starts) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                tracing::warn!(%error, "the cloud session's workspace could not be prepared");
                return self.toast(ToastKind::Error, &error.to_string(), cx);
            }
        };
        if let Some(problem) = &prepared.patch_problem {
            self.toast(ToastKind::Warning, problem, cx);
        }
        self.open_cloud_session(draft, prepared, starts, cx);
    }

    fn open_cloud_session(
        &mut self,
        draft: CloudDraft,
        prepared: PreparedWorkspace,
        starts: Vec<CarriedStart>,
        cx: &mut Context<Self>,
    ) {
        let CloudDraft {
            env, group, name, ..
        } = draft;
        if env == self.active {
            self.leave_main_overlays();
        }
        let Some(live) = self.envs.get_mut(&env) else {
            return;
        };
        if live.data.group(group).is_none() {
            return;
        }
        let runner = prepared.placement.runner;
        let session = live.data.new_session(Some(group), SessionSpec { name });
        if let Some(found) = live
            .data
            .sessions
            .iter_mut()
            .find(|found| found.id == session)
        {
            found.cloud = Some(prepared.placement);
        }
        live.data.show_session(session);
        live.selected_group = Some(group);
        tracing::info!(%runner, session = session.as_u64(), "cloud session opened");
        if starts.is_empty() {
            let preferred = self.settings.default_harness;
            let candidates = AgentPreset::chat_harnesses(&self.settings.acp.all_agents());
            let harness = cloud_chat_harness(preferred, &candidates);
            let chat = LiveSession::Start(new_session_id());
            self.spawn_chat_as(&env, NodeGroup::Exactly(Some(group)), harness, chat, cx);
        }
        for start in starts {
            self.spawn_seeded_chat(
                &env,
                Some(group),
                start.harness,
                start.session,
                start.prompts,
                cx,
            );
        }
        self.store();
        cx.notify();
    }
}
