use gpui_kit::{Context, PromptButton, PromptLevel};
use krad_chat::LiveSession;
use krad_domain::{CloudRunnerId, EnvironmentView, GroupId, SessionId, ToastKind};
use krad_protocol::{
    CarriedConversation, CloudCommand, CloudDraftId, CloudNews, CloudOpened, Command, PatchAnswer,
};

use super::super::native_alert::NATIVE_ALERTS;
use super::super::EnvironmentShell;
use super::state::runner_label;

const AT_LIMIT_TOAST: &str =
    "You already run as many cloud sessions as Settings allows; close one first";
const NO_REMOTE_TOAST: &str = "The group has no git remote to clone on the runner";
const REMOTE_UNREAD: &str = "Could not read the group's git remote";
const PREPARING_PREFIX: &str = "Preparing the cloud session on";
const RUNNER_FAILED_PREFIX: &str = "The cloud session could not start on";
const NO_SESSION_TOAST: &str = "Open a session first";
const ALREADY_CLOUD_TOAST: &str = "This session already runs in the cloud";
const NO_GROUP_TOAST: &str = "Only a session in a group can go to the cloud";
const NOT_CLOUD: &str = "This session does not run in the cloud";
const NOT_READY: &str = "The cloud runner is not connected yet; try again once it is ready";
const NO_REPO: &str = "The group's repository is not known yet";
const PUSHING: &str = "Pushing the session's branch from the runner…";
const PUSHED_PREFIX: &str = "Pushed";
const OPENING: &str = "Opening a pull request for the session…";
const OPENED_PREFIX: &str = "Opened";
const BRINGING: &str = "Bringing the session's branch to this computer…";
const BROUGHT_PREFIX: &str = "The branch is now a worktree group:";
pub(super) const PATCH_TITLE: &str = "Bring your uncommitted changes?";
pub(super) const PATCH_DETAIL: &str = "The cloud session starts from the group's branch on its remote. Your uncommitted changes, new files included, can be applied there as a patch.";
pub(super) const PATCH_CONFIRM: &str = "Bring them";
pub(super) const PATCH_LEAVE: &str = "Leave them";
pub(super) const PATCH_CANCEL: &str = "Cancel";
const BRING_ANSWER: usize = 0;
const LEAVE_ANSWER: usize = 1;

impl EnvironmentShell {
    fn post_cloud(&self, command: CloudCommand) {
        self.runtime.handle.post(Command::Cloud(command));
    }

    pub(in crate::environment_shell) fn new_cloud_session(
        &self,
        group: GroupId,
        runner: CloudRunnerId,
    ) {
        self.post_cloud(CloudCommand::Create {
            environment: self.active.clone(),
            group,
            runner,
        });
    }

    pub(in crate::environment_shell) fn send_session_to_cloud(
        &self,
        session: SessionId,
        runner: CloudRunnerId,
    ) {
        self.post_cloud(CloudCommand::Send {
            environment: self.active.clone(),
            session,
            runner,
        });
    }

    fn session_or_current(&self, session: Option<SessionId>) -> Option<SessionId> {
        session.or_else(|| {
            self.envs
                .get(&self.active)
                .and_then(|live| live.data.current_session())
                .map(|found| found.id)
        })
    }

    pub(in crate::environment_shell) fn cloud_result(
        &mut self,
        session: Option<SessionId>,
        command: impl FnOnce(krad_domain::EnvironmentName, SessionId) -> CloudCommand,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session_or_current(session) else {
            return self.toast(ToastKind::Warning, NO_SESSION_TOAST, cx);
        };
        self.post_cloud(command(self.active.clone(), session));
    }

    pub(in crate::environment_shell) fn cloud_patch_offered(
        &mut self,
        draft: CloudDraftId,
        cx: &mut Context<Self>,
    ) {
        let handle = self.attention.window_handle.filter(|_| NATIVE_ALERTS);
        let prompted = handle.and_then(|handle| {
            handle
                .update(cx, |_, window, cx| {
                    window.prompt(
                        PromptLevel::Warning,
                        PATCH_TITLE,
                        Some(PATCH_DETAIL),
                        &[
                            PromptButton::ok(PATCH_CONFIRM),
                            PromptButton::new(PATCH_LEAVE),
                            PromptButton::cancel(PATCH_CANCEL),
                        ],
                        cx,
                    )
                })
                .ok()
        });
        let Some(answer) = prompted else {
            self.cloud.asking = Some(draft);
            return cx.notify();
        };
        cx.spawn(async move |this, cx| {
            let answer = match answer.await {
                Ok(BRING_ANSWER) => PatchAnswer::Bring,
                Ok(LEAVE_ANSWER) => PatchAnswer::Leave,
                _ => PatchAnswer::Cancel,
            };
            this.update(cx, |shell, _| shell.answer_patch(draft, answer))
                .unwrap_or_default();
        })
        .detach();
    }

    pub(in crate::environment_shell) fn answer_patch(
        &self,
        draft: CloudDraftId,
        answer: PatchAnswer,
    ) {
        self.post_cloud(CloudCommand::AnswerPatch { draft, answer });
    }

    pub(in crate::environment_shell) fn cloud_news(
        &mut self,
        news: CloudNews,
        cx: &mut Context<Self>,
    ) {
        let (kind, text) = match news {
            CloudNews::AtLimit => (ToastKind::Warning, AT_LIMIT_TOAST.to_owned()),
            CloudNews::NoRemote => (ToastKind::Warning, NO_REMOTE_TOAST.to_owned()),
            CloudNews::NoSession => (ToastKind::Warning, NO_SESSION_TOAST.to_owned()),
            CloudNews::AlreadyCloud => (ToastKind::Warning, ALREADY_CLOUD_TOAST.to_owned()),
            CloudNews::NoGroup => (ToastKind::Warning, NO_GROUP_TOAST.to_owned()),
            CloudNews::NotCloud => (ToastKind::Warning, NOT_CLOUD.to_owned()),
            CloudNews::RunnerNotReady => (ToastKind::Warning, NOT_READY.to_owned()),
            CloudNews::NoRepo => (ToastKind::Warning, NO_REPO.to_owned()),
            CloudNews::RemoteUnread { error } => {
                (ToastKind::Error, format!("{REMOTE_UNREAD}: {error}"))
            }
            CloudNews::Preparing { runner } => (
                ToastKind::Info,
                format!("{PREPARING_PREFIX} {}…", self.runner_name(runner)),
            ),
            CloudNews::RunnerFailed { runner, reason } => (
                ToastKind::Error,
                format!(
                    "{RUNNER_FAILED_PREFIX} {}: {}",
                    self.runner_name(runner),
                    runner_label(&krad_domain::RunnerState::Failed { reason })
                ),
            ),
            CloudNews::NotPrepared { error } | CloudNews::ResultFailed { error } => {
                (ToastKind::Error, error)
            }
            CloudNews::PatchProblem { problem } => (ToastKind::Warning, problem),
            CloudNews::Pushing => (ToastKind::Info, PUSHING.to_owned()),
            CloudNews::Pushed { branch } => {
                (ToastKind::Success, format!("{PUSHED_PREFIX} {branch}"))
            }
            CloudNews::Opening => (ToastKind::Info, OPENING.to_owned()),
            CloudNews::RequestOpened { url } => {
                cx.open_url(&url);
                (ToastKind::Success, format!("{OPENED_PREFIX} {url}"))
            }
            CloudNews::Bringing => (ToastKind::Info, BRINGING.to_owned()),
            CloudNews::BroughtHome { branch } => {
                (ToastKind::Success, format!("{BROUGHT_PREFIX} {branch}"))
            }
            CloudNews::Unknown => return,
        };
        self.toast(kind, &text, cx);
    }

    pub(in crate::environment_shell) fn cloud_session_opened(
        &mut self,
        opened: CloudOpened,
        cx: &mut Context<Self>,
    ) {
        let CloudOpened {
            environment,
            group,
            starts,
            ..
        } = opened;
        if environment == self.active {
            self.leave_main_overlays();
        }
        let Some(live) = self.envs.get_mut(&environment) else {
            return;
        };
        live.data.set_view(EnvironmentView::Sessions);
        live.selected_group = Some(group);
        for start in starts {
            let session = match start.conversation {
                CarriedConversation::Fresh(resume) => LiveSession::Start(resume),
                CarriedConversation::Forked(resume) => LiveSession::Fork(resume),
            };
            self.spawn_seeded_chat(
                &environment,
                Some(group),
                start.harness,
                session,
                start.prompts,
                cx,
            );
        }
        self.store();
        cx.notify();
    }
}
