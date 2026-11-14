mod card;
mod ending;

use gpui_kit::Context;
use krad_domain::agents_api::{ErrorCode, Response};
use krad_domain::{
    new_session_id, worker_mode, AgentPreset, CloudPlacement, EnvironmentName, GroupId,
    LiveSession, NodeEdit, PermissionMode, Prompt, TerminalId, WorkerSpawn, WorkerSurface,
};
use krad_runtime::BusReply;

use super::agent_hooks::NO_GROUP;
use super::terminal_lifecycle::WorkerChat;
use super::EnvironmentShell;

pub(super) use card::{spawned_worker, WorkerCard, NEEDS_YOU_LABEL};
pub(super) use ending::LeadEnding;

const WORKER_FAILED: &str = "The worker could not start";
const WORKER_CLOSED_EARLY: &str = "The worker was closed before it started";
const NOT_A_WORKER: &str = "That agent cannot work as a worker";

pub(super) type LaunchReply = BusReply;

pub(super) struct WorkerLaunch {
    pub(super) group: Option<GroupId>,
    pub(super) preset: AgentPreset,
    pub(super) brief: String,
    pub(super) spawn: WorkerSpawn,
    pub(super) mode: Option<PermissionMode>,
    pub(super) cloud: Option<CloudPlacement>,
}

pub(super) enum LaunchProgress {
    Started,
    Failed(String),
    Unknown,
}

impl EnvironmentShell {
    pub(super) fn launch_worker(
        &mut self,
        env: &EnvironmentName,
        launch: WorkerLaunch,
        reply: LaunchReply,
        cx: &mut Context<Self>,
    ) {
        let WorkerLaunch {
            group,
            preset,
            brief,
            spawn,
            mode,
            cloud,
        } = launch;
        if preset.worker_surface() != WorkerSurface::Chat {
            reply
                .send(Response::error(ErrorCode::Unsupported, NOT_A_WORKER))
                .unwrap_or_default();
            return;
        }
        let chat = WorkerChat {
            group,
            preset,
            spawn,
            session: LiveSession::Start(new_session_id()),
            prompts: vec![Prompt::from(brief)],
            mode: worker_mode(mode, preset),
            cloud,
        };
        let opened = self
            .alloc_terminal_id(env)
            .and_then(|id| self.open_worker_chat(env, id, chat, cx));
        match opened.zip(self.envs.get_mut(env)) {
            Some((id, live)) => {
                live.worker_launches.insert(id, reply);
            }
            None => reply
                .send(Response::error(ErrorCode::Unsupported, NO_GROUP))
                .unwrap_or_default(),
        }
    }

    pub(super) fn follow_launch_progress(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        progress: LaunchProgress,
        cx: &mut Context<Self>,
    ) -> bool {
        let launching = self
            .envs
            .get(env)
            .is_some_and(|live| live.worker_launches.contains_key(&id));
        if !launching {
            return false;
        }
        match progress {
            LaunchProgress::Started => {
                self.answer_launch(
                    env,
                    id,
                    Response::Spawned {
                        terminal: id.as_u64(),
                    },
                );
                false
            }
            LaunchProgress::Failed(error) => {
                self.fail_worker_launch(env, id, &error, cx);
                true
            }
            LaunchProgress::Unknown => false,
        }
    }

    pub(super) fn fail_worker_launch(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        error: &str,
        cx: &mut Context<Self>,
    ) {
        let failure = Response::error(ErrorCode::Failed, format!("{WORKER_FAILED}: {error}"));
        if !self.answer_launch(env, id, failure) {
            return;
        }
        self.edit_environment(env, NodeEdit::ForgetWorker { terminal: id });
        self.close_terminal_in(env, id, cx);
    }

    pub(super) fn end_worker_launch(&mut self, env: &EnvironmentName, id: TerminalId) {
        self.answer_launch(
            env,
            id,
            Response::error(ErrorCode::Failed, WORKER_CLOSED_EARLY),
        );
    }

    fn answer_launch(&mut self, env: &EnvironmentName, id: TerminalId, response: Response) -> bool {
        let Some(reply) = self
            .envs
            .get_mut(env)
            .and_then(|live| live.worker_launches.remove(&id))
        else {
            return false;
        };
        reply.send(response).unwrap_or_default();
        true
    }

    pub(super) fn chat_permission_mode(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
    ) -> Option<PermissionMode> {
        self.envs
            .get(env)?
            .chats
            .get(&id)?
            .chat
            .structured()?
            .permission_mode
    }

    pub(super) fn is_launching(&self, env: &EnvironmentName, id: TerminalId) -> bool {
        self.envs
            .get(env)
            .is_some_and(|live| live.worker_launches.contains_key(&id))
    }

    pub(super) fn reopen_worker(
        &mut self,
        env: &EnvironmentName,
        terminal: TerminalId,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((lead, worker)) = self
            .envs
            .get(env)
            .and_then(|live| live.data.worker(terminal))
        else {
            return false;
        };
        if worker.session.is_some() {
            return true;
        }
        let Some(reopen) = worker.reopen.clone() else {
            return false;
        };
        let cloud = self
            .envs
            .get(env)
            .and_then(|live| live.data.session(lead))
            .and_then(|session| session.cloud.clone());
        let chat = WorkerChat {
            group: reopen.group,
            preset: reopen.harness,
            spawn: WorkerSpawn {
                lead: Some(lead),
                task: worker.task.clone(),
            },
            session: LiveSession::Resume(reopen.resume.clone()),
            prompts: Vec::new(),
            mode: None,
            cloud,
        };
        if self.open_worker_chat(env, terminal, chat, cx).is_none() {
            return false;
        }
        let resumed = NodeEdit::ResumeWorker {
            terminal,
            resume: reopen.resume,
        };
        self.edit_environment(env, resumed);
        self.store();
        true
    }
}
