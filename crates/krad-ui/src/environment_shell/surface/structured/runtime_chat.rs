mod heard;
mod shown;

use gpui_kit::App;
use krad_domain::{AgentState, Prompt, RewindScope, TerminalId};
use krad_protocol::{AgentRef, ChatCommand, Command};

use super::super::super::EnvironmentShell;
use super::super::{ChatPlace, SurfaceCommand};
use super::StructuredChat;

pub(super) use shown::RuntimeShown;

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

impl EnvironmentShell {
    pub(in crate::environment_shell) fn post_chat(&self, place: &ChatPlace, command: ChatCommand) {
        if let Some(client) = self.chat_client(place) {
            client.post(Command::Chat(command));
        } else {
            tracing::debug!("the chat's runtime is out of reach; the command is dropped");
        }
    }

    pub(in crate::environment_shell) fn send_prompt(
        &mut self,
        place: &ChatPlace,
        id: TerminalId,
        prompt: Prompt,
        cx: &mut App,
    ) {
        let chat = place.chat(id);
        self.post_chat(place, ChatCommand::Send { chat, prompt });
        if let Some(env) = place.local().cloned() {
            self.set_agent_state(&env, id, AgentState::Working, cx);
        }
    }

    pub(in crate::environment_shell) fn command_runtime_chat(
        &self,
        place: &ChatPlace,
        id: TerminalId,
        command: &SurfaceCommand,
    ) {
        if structured_of(self, place, id).is_none() {
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
            SurfaceCommand::ToggleThinking => ChatCommand::SetThinking {
                chat,
                thinking: structured_of(self, place, id).is_some_and(|chat| !chat.thinking),
            },
        };
        self.post_chat(place, posted);
    }

    pub(in crate::environment_shell) fn post_runtime_chat(
        &self,
        place: &ChatPlace,
        id: TerminalId,
        command: impl FnOnce(AgentRef) -> ChatCommand,
    ) -> bool {
        if structured_of(self, place, id).is_none() {
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

    pub(in crate::environment_shell) fn close_runtime_chat(
        &mut self,
        place: &ChatPlace,
        id: TerminalId,
        cx: &mut App,
    ) {
        let chat = place.chat(id);
        if !self.runtime.chats.contains(&chat) {
            return;
        }
        self.runtime.edit_chats(cx, |chats| chats.forget(&chat));
        self.post_chat(place, ChatCommand::Close { chat });
    }
}

fn structured_of<'a>(
    shell: &'a EnvironmentShell,
    place: &ChatPlace,
    id: TerminalId,
) -> Option<&'a StructuredChat> {
    shell.live(place)?.chats.get(&id)?.chat.structured()
}
