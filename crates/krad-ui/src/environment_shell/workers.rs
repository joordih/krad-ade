mod card;
mod ending;
mod mode;

use futures::channel::oneshot::Sender as ReplySender;
use gpui_kit::Context;
use krad_agents::Response;
use krad_chat::{new_session_id, LiveEvent, LiveSession};
use krad_domain::{
    AgentPreset, ChatEvent, EnvironmentName, GroupId, PermissionMode, Prompt, TerminalId,
    WorkerSpawn, WorkerSurface,
};

use super::agent_hooks::NO_GROUP;
use super::terminal_lifecycle::WorkerChat;
use super::EnvironmentShell;

pub(super) use card::{spawned_worker, WorkerCard, NEEDS_YOU_LABEL};
pub(super) use ending::LeadEnding;

const WORKER_FAILED: &str = "The worker could not start";
const WORKER_CLOSED_EARLY: &str = "The worker was closed before it started";
const NOT_A_WORKER: &str = "That agent cannot work as a worker";

pub(super) type LaunchReply = ReplySender<Response>;

pub(super) struct WorkerLaunch {
    pub(super) group: Option<GroupId>,
    pub(super) preset: AgentPreset,
    pub(super) brief: String,
    pub(super) spawn: WorkerSpawn,
    pub(super) mode: Option<PermissionMode>,
}

enum LaunchProgress {
    Started,
    Failed(String),
    Unknown,
}

fn launch_progress(event: &LiveEvent) -> LaunchProgress {
    match event {
        LiveEvent::Chat(ChatEvent::Error(error)) => LaunchProgress::Failed(error.clone()),
        LiveEvent::Refused(refused) => LaunchProgress::Failed(refused.to_string()),
        LiveEvent::Chat(_) | LiveEvent::Ready(_) | LiveEvent::Ask(_) => LaunchProgress::Started,
        LiveEvent::Usage(_) | LiveEvent::Notice(_) | LiveEvent::Rewound(_) | LiveEvent::Acp(_) => {
            LaunchProgress::Unknown
        }
    }
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
        } = launch;
        if preset.worker_surface() != WorkerSurface::Chat {
            reply
                .send(Response::Error(NOT_A_WORKER.to_owned()))
                .unwrap_or_default();
            return;
        }
        let chat = WorkerChat {
            group,
            preset,
            spawn,
            session: LiveSession::Start(new_session_id()),
            prompts: vec![Prompt::from(brief)],
            mode: mode::worker_mode(mode, preset),
        };
        let opened = self
            .alloc_terminal_id(env)
            .and_then(|id| self.open_worker_chat(env, id, chat, cx));
        match opened.zip(self.envs.get_mut(env)) {
            Some((id, live)) => {
                live.worker_launches.insert(id, reply);
            }
            None => reply
                .send(Response::Error(NO_GROUP.to_owned()))
                .unwrap_or_default(),
        }
    }

    pub(super) fn follow_worker_launch(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        event: &LiveEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let launching = self
            .envs
            .get(env)
            .is_some_and(|live| live.worker_launches.contains_key(&id));
        if !launching {
            return false;
        }
        match launch_progress(event) {
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
        let failure = Response::Error(format!("{WORKER_FAILED}: {error}"));
        if !self.answer_launch(env, id, failure) {
            return;
        }
        if let Some(live) = self.envs.get_mut(env) {
            live.data.forget_worker(id);
        }
        self.close_terminal_in(env, id, cx);
    }

    pub(super) fn end_worker_launch(&mut self, env: &EnvironmentName, id: TerminalId) {
        self.answer_launch(env, id, Response::Error(WORKER_CLOSED_EARLY.to_owned()));
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
        };
        if self.open_worker_chat(env, terminal, chat, cx).is_none() {
            return false;
        }
        if let Some(node) = self
            .envs
            .get_mut(env)
            .and_then(|live| live.data.nodes.iter_mut().find(|node| node.id == terminal))
        {
            node.resume = Some(reopen.resume);
        }
        self.store();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_worker_has_started_once_its_harness_says_anything_but_an_error() {
        let started = launch_progress(&LiveEvent::Chat(ChatEvent::Text("On it".to_owned())));
        assert!(matches!(started, LaunchProgress::Started));
        let failed = launch_progress(&LiveEvent::Chat(ChatEvent::Error("no claude".to_owned())));
        assert!(matches!(failed, LaunchProgress::Failed(error) if error == "no claude"));
        let notice = launch_progress(&LiveEvent::Notice("slow".to_owned()));
        assert!(matches!(notice, LaunchProgress::Unknown));
    }
}
