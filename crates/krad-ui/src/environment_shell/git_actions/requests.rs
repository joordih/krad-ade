use gpui_kit::Context;
use krad_agents::{ErrorCode, Response};
use krad_domain::{EnvironmentName, GroupId, ToastKind};
use krad_protocol::{Command, GitFinished, GitOutcome, Reply, RequestId};
use krad_runtime::BusReply;
use std::collections::HashMap;

use super::super::sessions::Launch;
use super::super::workers::WorkerLaunch;
use super::super::EnvironmentShell;

const STALE_TOAST: &str = "The file changed since its diff was read; try again.";

pub(in crate::environment_shell) enum GitFollowUp {
    Told {
        told: Option<&'static str>,
        clears_message: bool,
    },
    SelectGroup,
    SpawnIn {
        label: String,
        launch: Launch,
    },
    LaunchWorker {
        launch: Box<WorkerLaunch>,
        reply: BusReply,
    },
    ReloadSessionChanges,
}

#[derive(Default)]
pub(in crate::environment_shell) struct GitRequests(HashMap<RequestId, GitFollowUp>);

impl EnvironmentShell {
    pub(in crate::environment_shell) fn post_git(
        &mut self,
        command: Command,
        follow: GitFollowUp,
        cx: &mut Context<Self>,
    ) {
        let request = self.runtime.client.next_request();
        self.git_requests.0.insert(request, follow);
        let pending = self.runtime.client.send_as(request, command);
        cx.spawn(async move |this, cx| {
            let Ok(Reply::Refused { message, .. }) = pending.await else {
                return;
            };
            this.update(cx, |shell, cx| shell.git_refused(request, &message, cx))
                .unwrap_or_default();
        })
        .detach();
    }

    fn git_refused(&mut self, request: RequestId, message: &str, cx: &mut Context<Self>) {
        let Some(follow) = self.git_requests.0.remove(&request) else {
            return;
        };
        match follow {
            GitFollowUp::LaunchWorker { reply, .. } => {
                reply
                    .send(Response::error(ErrorCode::BadRequest, message))
                    .unwrap_or_default();
            }
            GitFollowUp::Told { .. }
            | GitFollowUp::SelectGroup
            | GitFollowUp::SpawnIn { .. }
            | GitFollowUp::ReloadSessionChanges => {
                self.toast(ToastKind::Warning, message, cx);
            }
        }
    }

    pub(in crate::environment_shell) fn git_finished(
        &mut self,
        finished: GitFinished,
        cx: &mut Context<Self>,
    ) {
        let Some(follow) = self.git_requests.0.remove(&finished.request) else {
            return;
        };
        let env = finished.environment;
        let opened = match finished.outcome {
            GitOutcome::Failed { message } => {
                self.git_failed(follow, &message, cx);
                return;
            }
            GitOutcome::Stale => {
                self.toast(ToastKind::Warning, STALE_TOAST, cx);
                self.reload_session_changes(cx);
                return;
            }
            GitOutcome::Opened { group } => Some(group),
            GitOutcome::Done | GitOutcome::Unknown => None,
        };
        self.git_done(&env, follow, opened, cx);
    }

    fn git_failed(&mut self, follow: GitFollowUp, message: &str, cx: &mut Context<Self>) {
        match follow {
            GitFollowUp::LaunchWorker { reply, .. } => {
                reply
                    .send(Response::error(ErrorCode::Failed, message))
                    .unwrap_or_default();
            }
            GitFollowUp::ReloadSessionChanges => {
                self.toast(ToastKind::Error, message, cx);
                self.reload_session_changes(cx);
            }
            GitFollowUp::Told { .. } | GitFollowUp::SelectGroup | GitFollowUp::SpawnIn { .. } => {
                self.toast(ToastKind::Error, message, cx);
            }
        }
    }

    fn git_done(
        &mut self,
        env: &EnvironmentName,
        follow: GitFollowUp,
        opened: Option<GroupId>,
        cx: &mut Context<Self>,
    ) {
        match follow {
            GitFollowUp::Told {
                told,
                clears_message,
            } => {
                if clears_message {
                    self.git_ui.clear_message();
                }
                if let Some(told) = told {
                    self.toast(ToastKind::Info, told, cx);
                }
            }
            GitFollowUp::SelectGroup => {
                if let Some(group) = opened {
                    self.select_opened_group(env, group, cx);
                }
            }
            GitFollowUp::SpawnIn { label, launch } => {
                self.spawn_terminal_in(env, opened, &label, launch, None, cx);
            }
            GitFollowUp::LaunchWorker { launch, reply } => {
                let launch = WorkerLaunch {
                    group: opened,
                    ..*launch
                };
                self.launch_worker(env, launch, reply, cx);
            }
            GitFollowUp::ReloadSessionChanges => self.reload_session_changes(cx),
        }
    }

    fn select_opened_group(
        &mut self,
        env: &EnvironmentName,
        group: GroupId,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.envs.get_mut(env) {
            live.selected_group = Some(group);
        }
        if *env == self.active {
            self.select_group(group, cx);
        }
    }
}
