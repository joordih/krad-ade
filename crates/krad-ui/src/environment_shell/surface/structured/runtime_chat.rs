mod heard;
mod shown;

use gpui_kit::{App, Context};
use krad_chat::SlashCommand;
use krad_domain::{AgentState, EnvironmentName, Prompt, RewindScope, TerminalId};
use krad_protocol::{AgentRef, ChatCommand, ChatOpen, Command};
use krad_runtime::runs_in_runtime;

use super::super::super::chat::StartError;
use super::super::super::EnvironmentShell;
use super::super::{Chat, ChatView, SurfaceCommand};
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

pub(super) fn chat_ref(env: &EnvironmentName, id: TerminalId) -> AgentRef {
    AgentRef {
        environment: env.clone(),
        terminal: id,
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
    pub(super) fn opens_in_runtime(&self, env: &EnvironmentName, id: TerminalId) -> bool {
        structured_of(self, env, id).is_some_and(|structured| {
            runs_in_runtime(structured.harness) && !structured.in_runtime()
        })
    }

    pub(super) fn open_in_runtime(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        cx: &App,
    ) -> Result<(), StartError> {
        if self.runtime_chat_waits(env, cx) {
            return Err(StartError::Waiting);
        }
        let Some(harness) = structured_of(self, env, id).map(|structured| structured.harness)
        else {
            return Ok(());
        };
        let Some(Chat {
            backend: super::super::Backend::Structured(structured),
            messages,
            ..
        }) = structured_chat(self, env, id).map(|view| &mut view.chat)
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
        let chat = chat_ref(env, id);
        self.runtime.client.post(Command::Chat(ChatCommand::Open {
            chat,
            start: Box::new(start),
        }));
        Ok(())
    }

    fn runtime_chat_waits(&self, env: &EnvironmentName, cx: &App) -> bool {
        let link = self.env_link(env);
        let agents_unknown = !link.is_local() && !self.host_agents.contains_key(&link.host());
        self.awaits_remote_bus(env, cx) || !link.is_ready() || agents_unknown
    }

    pub(super) fn hand_to_runtime(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        commands: &[SlashCommand],
        prompts: Vec<Prompt>,
    ) {
        let chat = chat_ref(env, id);
        let starts_turn = !prompts.is_empty();
        for command in batch_commands(&chat, commands, prompts) {
            self.runtime.client.post(Command::Chat(command));
        }
        if starts_turn {
            self.set_agent_state(env, id, AgentState::Working);
        }
    }

    pub(in crate::environment_shell) fn command_runtime_chat(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
        command: &SurfaceCommand,
    ) {
        if !structured_of(self, env, id).is_some_and(StructuredChat::in_runtime) {
            return;
        }
        let chat = chat_ref(env, id);
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
        self.runtime.client.post(Command::Chat(posted));
    }

    pub(in crate::environment_shell) fn post_runtime_chat(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
        command: impl FnOnce(AgentRef) -> ChatCommand,
    ) -> bool {
        if !structured_of(self, env, id).is_some_and(StructuredChat::in_runtime) {
            return false;
        }
        self.runtime
            .client
            .post(Command::Chat(command(chat_ref(env, id))));
        true
    }

    pub(super) fn rewind_in_runtime(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
        (index, prompt): (usize, Prompt),
        scope: RewindScope,
    ) -> bool {
        let from = structured_of(self, env, id).map_or(0, StructuredChat::runtime_from);
        self.post_runtime_chat(env, id, |chat| ChatCommand::Rewind {
            chat,
            index: index + from,
            prompt,
            scope,
        })
    }

    pub(in crate::environment_shell) fn leave_runtime(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
    ) {
        let Some(structured) = structured_mut(self, env, id).filter(|chat| chat.in_runtime())
        else {
            return;
        };
        structured.driver = ChatDriver::InShell;
        self.post_close_chat(env, id);
    }

    pub(in crate::environment_shell) fn close_runtime_chat(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
        chat: Option<&ChatView>,
    ) {
        if chat
            .and_then(|view| view.chat.structured())
            .is_some_and(StructuredChat::in_runtime)
        {
            self.post_close_chat(env, id);
        }
    }

    fn post_close_chat(&self, env: &EnvironmentName, id: TerminalId) {
        let chat = chat_ref(env, id);
        self.runtime
            .client
            .post(Command::Chat(ChatCommand::Close { chat }));
    }

    pub(in crate::environment_shell) fn runtime_chat_opened(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) {
        if !self.opens_in_runtime(env, id) {
            return;
        }
        let resume = structured_of(self, env, id)
            .and_then(|structured| structured.session.history().cloned());
        match (self.open_in_runtime(env, id, cx), resume) {
            (Ok(()), _) | (Err(StartError::Waiting), None) => {}
            (Err(StartError::Waiting), Some(_)) => self.note_history_waits_for_host(env, id),
            (Err(error), _) => {
                if let Some(chat) = structured_chat(self, env, id) {
                    chat.chat.problem = Some(error.to_string());
                }
            }
        }
        cx.notify();
    }
}

fn structured_of<'a>(
    shell: &'a EnvironmentShell,
    env: &EnvironmentName,
    id: TerminalId,
) -> Option<&'a StructuredChat> {
    shell.envs.get(env)?.chats.get(&id)?.chat.structured()
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{Effort, ModelId};

    #[test]
    fn a_batch_reaches_the_runtime_as_settings_then_every_message() {
        let chat = chat_ref(&EnvironmentName::new("main".to_owned()), TerminalId(3));
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
