use gpui_kit::{App, Context};
use krad_domain::{AgentPreset, EnvironmentName, TerminalId};

use super::super::super::chat::StartError;
use super::super::super::EnvironmentShell;
use super::super::{Backend, Chat, ChatPlace};
use super::{structured_chat, structured_mut};

impl EnvironmentShell {
    pub(in crate::environment_shell::surface) fn flush_structured(
        &mut self,
        place: &ChatPlace,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) {
        let has_work =
            structured_chat(self, place, id).is_some_and(|chat| match &chat.chat.backend {
                Backend::Structured(structured) => structured.has_work(&chat.chat.outbox),
                Backend::Transcript(_) => false,
            });
        if !has_work || !self.start_or_report(place, id, cx) {
            return;
        }
        let Some(Chat {
            backend: Backend::Structured(structured),
            outbox,
            ..
        }) = structured_chat(self, place, id).map(|view| &mut view.chat)
        else {
            return;
        };
        if !structured.in_runtime() {
            return;
        }
        let commands = std::mem::take(&mut structured.commands);
        let prompts = outbox.take_queued();
        self.hand_to_runtime(place, id, &commands, prompts);
    }

    pub(super) fn start_or_report(
        &mut self,
        place: &ChatPlace,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) -> bool {
        let Err(error) = self.ensure_live(place, id, cx) else {
            return true;
        };
        if matches!(error, StartError::Waiting) {
            return false;
        }
        if let Some(chat) = self
            .live_mut(place)
            .and_then(|live| live.chats.get_mut(&id))
        {
            chat.chat.problem = Some(error.to_string());
        }
        if let Some(env) = place.local() {
            let env = env.clone();
            self.fail_worker_launch(&env, id, &error.to_string(), cx);
        }
        false
    }

    fn ensure_live(
        &mut self,
        place: &ChatPlace,
        id: TerminalId,
        cx: &App,
    ) -> Result<(), StartError> {
        let Some(harness) = structured_mut(self, place, id)
            .filter(|structured| !structured.in_runtime())
            .map(|structured| structured.harness)
        else {
            return Ok(());
        };
        if self.opens_in_runtime(place, id) {
            return self.open_in_runtime(place, id, cx);
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
            let place = ChatPlace::here(&env);
            self.runtime_chat_opened(&place, id, cx);
            self.flush_structured(&place, id, cx);
        }
    }
}
