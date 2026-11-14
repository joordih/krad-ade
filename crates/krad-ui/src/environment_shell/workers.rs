mod card;
mod ending;

use gpui_kit::Context;
use krad_domain::{EnvironmentName, LiveSession, NodeEdit, TerminalId, WorkerSpawn};

use super::terminal_lifecycle::WorkerChat;
use super::EnvironmentShell;

pub(super) use card::{spawned_worker, WorkerCard, NEEDS_YOU_LABEL};
pub(super) use ending::LeadEnding;

impl EnvironmentShell {
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
        self.edit_environment(env, resumed, cx);
        self.save_view();
        true
    }
}
