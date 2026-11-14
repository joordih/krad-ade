mod heard;
mod shown;

use gpui_kit::{App, Context};
use krad_domain::SlashCommand;
use krad_domain::{AgentState, Prompt, RewindScope, TerminalId};
use krad_protocol::{AgentRef, ChatCommand, ChatOpen, Command};
use krad_runtime::runs_in_runtime;

use super::super::super::chat::StartError;
use super::super::super::EnvironmentShell;
use super::super::{Chat, ChatPlace, ChatView, SurfaceCommand};
use super::{structured_chat, structured_mut, StructuredChat};

pub(super) use shown::RuntimeShown;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::environment_shell) enum ChatDriver {
    #[default]
    InShell,
    InRuntime,
}

impl StructuredChat {
    pub(in crate::environment_shell) const fn in_runtime(&self) -> bool {
        matches!(self.driver, ChatDriver::InRuntime)
    }
}

pub(super) fn rewind_command(
    chat: AgentRef,
    (index, prompt): (usize, Prompt),
    from: usize,
    scope: RewindScope,
) -> ChatCommand {
    ChatCommand::Rewind {
        chat,
        index: index + from,
        prompt,
        scope,
    }
}

fn batch_commands(
    chat: &AgentRef,
    commands: &[SlashCommand],
    prompts: Vec<Prompt>,
) -> Vec<ChatCommand> {
    commands
        .iter()
        .map(|command| match command {
            SlashCommand::Model(model) => ChatCommand::SetModel {
                chat: chat.clone(),
                model: model.clone(),
            },
            SlashCommand::Effort(effort) => ChatCommand::SetEffort {
                chat: chat.clone(),
                effort: *effort,
            },
            SlashCommand::Thinking(thinking) => ChatCommand::SetThinking {
                chat: chat.clone(),
                thinking: *thinking,
            },
        })
        .chain(prompts.into_iter().map(|prompt| ChatCommand::Send {
            chat: chat.clone(),
            prompt,
        }))
        .collect()
}

impl EnvironmentShell {
    pub(super) fn opens_in_runtime(&self, place: &ChatPlace, id: TerminalId) -> bool {
        structured_of(self, place, id).is_some_and(|structured| {
            runs_in_runtime(structured.harness) && !structured.in_runtime()
        })
    }

    pub(super) fn open_in_runtime(
        &mut self,
        place: &ChatPlace,
        id: TerminalId,
        cx: &App,
    ) -> Result<(), StartError> {
        if self.runtime_chat_waits(place, cx) {
            return Err(StartError::Waiting);
        }
        let Some(harness) = structured_of(self, place, id).map(|structured| structured.harness)
        else {
            return Ok(());
        };
        let Some(Chat {
            backend: super::super::Backend::Structured(structured),
            messages,
            ..
        }) = structured_chat(self, place, id).map(|view| &mut view.chat)
        else {
            return Ok(());
        };
        let start = ChatOpen {
            harness,
            model: structured.chosen_model.clone(),
            effort: structured.effort,
            permission_mode: structured.permission_mode,
            mcp_servers: structured.mcp_servers.clone(),
            acp: structured.acp.wanted(),
            session: structured.session.clone(),
            messages: messages.clone(),
            notices: std::mem::take(&mut structured.notices),
        };
        structured.driver = ChatDriver::InRuntime;
        let chat = place.chat(id);
        self.post_chat(
            place,
            ChatCommand::Open {
                chat,
                start: Box::new(start),
            },
        );
        Ok(())
    }

    fn runtime_chat_waits(&self, place: &ChatPlace, cx: &App) -> bool {
        let Some(env) = place.local() else {
            return self.chat_client(place).is_none();
        };
        let link = self.env_link(env);
        let agents_unknown = !link.is_local() && self.agents(cx).on_host(&link.host()).is_none();
        self.awaits_remote_bus(env, cx) || !link.is_ready() || agents_unknown
    }

    pub(in crate::environment_shell) fn post_chat(&self, place: &ChatPlace, command: ChatCommand) {
        if let Some(client) = self.chat_client(place) {
            client.post(Command::Chat(command));
        } else {
            tracing::debug!("the chat's runtime is out of reach; the command is dropped");
        }
    }

    pub(super) fn hand_to_runtime(
        &mut self,
        place: &ChatPlace,
        id: TerminalId,
        commands: &[SlashCommand],
        prompts: Vec<Prompt>,
    ) {
        let chat = place.chat(id);
        let starts_turn = !prompts.is_empty();
        for command in batch_commands(&chat, commands, prompts) {
            self.post_chat(place, command);
        }
        if let Some(env) = place.local().filter(|_| starts_turn) {
            self.set_agent_state(env, id, AgentState::Working);
        }
    }

    pub(in crate::environment_shell) fn command_runtime_chat(
        &self,
        place: &ChatPlace,
        id: TerminalId,
        command: &SurfaceCommand,
    ) {
        if !structured_of(self, place, id).is_some_and(StructuredChat::in_runtime) {
            return;
        }
        let chat = place.chat(id);
        let posted = match command {
            SurfaceCommand::Stop => ChatCommand::Interrupt { chat },
            SurfaceCommand::Model(_, model) => ChatCommand::SetModel {
                chat,
                model: model.clone(),
            },
            SurfaceCommand::Effort(effort) => ChatCommand::SetEffort {
                chat,
                effort: *effort,
            },
            SurfaceCommand::ToggleThinking => return,
        };
        self.post_chat(place, posted);
    }

    pub(in crate::environment_shell) fn post_runtime_chat(
        &self,
        place: &ChatPlace,
        id: TerminalId,
        command: impl FnOnce(AgentRef) -> ChatCommand,
    ) -> bool {
        if !structured_of(self, place, id).is_some_and(StructuredChat::in_runtime) {
            return false;
        }
        self.post_chat(place, command(place.chat(id)));
        true
    }

    pub(super) fn rewind_in_runtime(
        &self,
        place: &ChatPlace,
        id: TerminalId,
        (index, prompt): (usize, Prompt),
        scope: RewindScope,
    ) -> bool {
        let from = structured_of(self, place, id).map_or(0, StructuredChat::runtime_from);
        self.post_runtime_chat(place, id, |chat| {
            rewind_command(chat, (index, prompt), from, scope)
        })
    }

    pub(in crate::environment_shell) fn leave_runtime(
        &mut self,
        place: &ChatPlace,
        id: TerminalId,
    ) {
        let Some(structured) = structured_mut(self, place, id).filter(|chat| chat.in_runtime())
        else {
            return;
        };
        structured.driver = ChatDriver::InShell;
        self.post_close_chat(place, id);
    }

    pub(in crate::environment_shell) fn close_runtime_chat(
        &self,
        place: &ChatPlace,
        id: TerminalId,
        chat: Option<&ChatView>,
    ) {
        if chat
            .and_then(|view| view.chat.structured())
            .is_some_and(StructuredChat::in_runtime)
        {
            self.post_close_chat(place, id);
        }
    }

    fn post_close_chat(&self, place: &ChatPlace, id: TerminalId) {
        self.post_chat(
            place,
            ChatCommand::Close {
                chat: place.chat(id),
            },
        );
    }

    pub(in crate::environment_shell) fn runtime_chat_opened(
        &mut self,
        place: &ChatPlace,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) {
        if !self.opens_in_runtime(place, id) {
            return;
        }
        let resume = structured_of(self, place, id)
            .and_then(|structured| structured.session.history().cloned());
        match (self.open_in_runtime(place, id, cx), resume) {
            (Ok(()), _) | (Err(StartError::Waiting), None) => {}
            (Err(StartError::Waiting), Some(_)) => self.note_history_waits_for_host(place, id),
            (Err(error), _) => {
                if let Some(chat) = structured_chat(self, place, id) {
                    chat.chat.problem = Some(error.to_string());
                }
            }
        }
        cx.notify();
    }
}

fn structured_of<'a>(
    shell: &'a EnvironmentShell,
    place: &ChatPlace,
    id: TerminalId,
) -> Option<&'a StructuredChat> {
    shell.live(place)?.chats.get(&id)?.chat.structured()
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{Effort, ModelId};

    #[test]
    fn a_batch_reaches_the_runtime_as_settings_then_every_message() {
        let chat = ChatPlace::here(&krad_domain::EnvironmentName::new("main".to_owned()))
            .chat(TerminalId(3));
        let model = ModelId::parse("opus");
        let commands: Vec<SlashCommand> = model
            .clone()
            .map(SlashCommand::Model)
            .into_iter()
            .chain([
                SlashCommand::Thinking(false),
                SlashCommand::Effort(Effort::High),
            ])
            .collect();
        let prompts = vec![Prompt::from("go"), Prompt::from("then this")];
        let expected: Vec<ChatCommand> = model
            .map(|model| ChatCommand::SetModel {
                chat: chat.clone(),
                model,
            })
            .into_iter()
            .chain([
                ChatCommand::SetThinking {
                    chat: chat.clone(),
                    thinking: false,
                },
                ChatCommand::SetEffort {
                    chat: chat.clone(),
                    effort: Effort::High,
                },
                ChatCommand::Send {
                    chat: chat.clone(),
                    prompt: Prompt::from("go"),
                },
                ChatCommand::Send {
                    chat: chat.clone(),
                    prompt: Prompt::from("then this"),
                },
            ])
            .collect();
        assert_eq!(batch_commands(&chat, &commands, prompts), expected);
    }
}
