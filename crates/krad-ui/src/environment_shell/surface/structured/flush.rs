use gpui_kit::Context;
use krad_domain::{AgentPreset, EnvironmentName, TerminalId};

use super::super::super::chat::StartError;
use super::super::super::EnvironmentShell;
use super::super::{Backend, Chat};
use super::{structured_chat, structured_mut};

impl EnvironmentShell {
    pub(in crate::environment_shell::surface) fn flush_structured(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) {
        let has_work =
            structured_chat(self, env, id).is_some_and(|chat| match &chat.chat.backend {
                Backend::Structured(structured) => structured.has_work(&chat.chat.outbox),
                Backend::Transcript(_) => false,
            });
        if !has_work || !self.start_or_report(env, id, cx) {
            return;
        }
        let Some(Chat {
            backend: Backend::Structured(structured),
            outbox,
            ..
        }) = structured_chat(self, env, id).map(|view| &mut view.chat)
        else {
            return;
        };
        if !structured.in_runtime() {
            return;
        }
        let commands = std::mem::take(&mut structured.commands);
        let prompts = outbox.take_queued();
        self.hand_to_runtime(env, id, &commands, prompts);
    }

    pub(super) fn start_or_report(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) -> bool {
        let Err(error) = self.ensure_live(env, id) else {
            return true;
        };
        if matches!(error, StartError::Waiting) {
            return false;
        }
        if let Some(chat) = self
            .envs
            .get_mut(env)
            .and_then(|live| live.chats.get_mut(&id))
        {
            chat.chat.problem = Some(error.to_string());
        }
        self.fail_worker_launch(env, id, &error.to_string(), cx);
        false
    }

    fn ensure_live(&mut self, env: &EnvironmentName, id: TerminalId) -> Result<(), StartError> {
        let Some(harness) = structured_mut(self, env, id)
            .filter(|structured| !structured.in_runtime())
            .map(|structured| structured.harness)
        else {
            return Ok(());
        };
        if self.opens_in_runtime(env, id) {
            return self.open_in_runtime(env, id);
        }
        Err(StartError::NoChat(self.settings.acp.harness_name(harness)))
    }

    pub(in crate::environment_shell) fn flush_chats_of(
        &mut self,
        harness: AgentPreset,
        cx: &mut Context<Self>,
    ) {
        let waiting: Vec<(EnvironmentName, TerminalId)> = self
            .envs
            .iter()
            .flat_map(|(env, live)| {
                live.chats
                    .iter()
                    .filter(|(_, chat)| {
                        chat.chat
                            .structured()
                            .is_some_and(|structured| structured.harness == harness)
                    })
                    .map(move |(id, _)| (env.clone(), *id))
            })
            .collect();
        for (env, id) in waiting {
            self.runtime_chat_opened(&env, id, cx);
            self.flush_structured(&env, id, cx);
        }
    }
}
