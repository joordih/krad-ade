mod heard;

use gpui_kit::Context;
use krad_chat::SlashCommand;
use krad_domain::{AgentPreset, AgentState, EnvironmentName, Prompt, RewindScope, TerminalId};
use krad_protocol::{AgentRef, ChatCommand, Command};
use krad_runtime::{runs_in_runtime, ChatPlace, ChatStart};

use super::super::super::chat::StartError;
use super::super::super::cloud::NodeSite;
use super::super::super::EnvironmentShell;
use super::super::{SurfaceChat, SurfaceCommand};
use super::{structured_chat, structured_mut, Batch, StructuredChat};

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

fn chat_ref(env: &EnvironmentName, id: TerminalId) -> AgentRef {
    AgentRef {
        environment: env.clone(),
        terminal: id,
    }
}

fn batch_commands(chat: &AgentRef, batch: &Batch) -> Vec<ChatCommand> {
    let mut commands: Vec<ChatCommand> = batch
        .commands
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
        .collect();
    commands.extend(batch.message.clone().map(|prompt| ChatCommand::Send {
        chat: chat.clone(),
        prompt,
    }));
    commands
}

impl EnvironmentShell {
    pub(super) fn opens_in_runtime(&self, env: &EnvironmentName, id: TerminalId) -> bool {
        let runs = structured_of(self, env, id).is_some_and(|structured| {
            runs_in_runtime(structured.harness) && !structured.in_runtime()
        });
        runs && matches!(self.node_site(env, id), NodeSite::Env)
    }

    pub(super) fn open_in_runtime(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
    ) -> Result<(), StartError> {
        if self.awaits_remote_bus(env) {
            return Err(StartError::Waiting);
        }
        let harness = AgentPreset::ClaudeCode;
        let config = self.live_config(env, id, harness)?;
        let link = self.env_link(env);
        let root = self
            .envs
            .get(env)
            .and_then(|live| live.data.group_of(id))
            .map(|group| group.root.clone());
        let Some(SurfaceChat {
            backend: super::super::Backend::Structured(structured),
            messages,
            ..
        }) = structured_chat(self, env, id)
        else {
            return Ok(());
        };
        let start = ChatStart {
            harness,
            config,
            session: structured.session.clone(),
            place: ChatPlace {
                host: link.host(),
                link,
                root,
            },
            messages: messages.clone(),
            queued: Vec::new(),
        };
        structured.driver = ChatDriver::InRuntime;
        self.runtime.handle.open_chat(chat_ref(env, id), start);
        Ok(())
    }

    pub(super) fn hand_to_runtime(&mut self, env: &EnvironmentName, id: TerminalId, batch: &Batch) {
        let chat = chat_ref(env, id);
        for command in batch_commands(&chat, batch) {
            self.runtime.handle.post(Command::Chat(command));
        }
        if batch.message.is_some() {
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
        self.runtime.handle.post(Command::Chat(posted));
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
            .handle
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
        self.post_runtime_chat(env, id, |chat| ChatCommand::Rewind {
            chat,
            index,
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
        self.runtime.handle.close_chat(chat_ref(env, id));
    }

    pub(in crate::environment_shell) fn close_runtime_chat(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
        chat: Option<&SurfaceChat>,
    ) {
        if chat
            .and_then(SurfaceChat::structured)
            .is_some_and(StructuredChat::in_runtime)
        {
            self.runtime.handle.close_chat(chat_ref(env, id));
        }
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
        match (self.open_in_runtime(env, id), resume) {
            (Ok(()), _) | (Err(StartError::Waiting), None) => {}
            (Err(StartError::Waiting), Some(_)) => self.note_history_waits_for_host(env, id),
            (Err(_), Some(resume)) => {
                self.load_history(env.clone(), id, AgentPreset::ClaudeCode, resume, cx);
            }
            (Err(error), None) => {
                if let Some(chat) = structured_chat(self, env, id) {
                    chat.problem = Some(error.to_string());
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
    shell.envs.get(env)?.chats.get(&id)?.structured()
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{Effort, ModelId};

    #[test]
    fn a_batch_reaches_the_runtime_as_settings_then_the_message() {
        let chat = chat_ref(&EnvironmentName::new("main".to_owned()), TerminalId(3));
        let model = ModelId::parse("opus");
        let batch = Batch {
            commands: model
                .clone()
                .map(SlashCommand::Model)
                .into_iter()
                .chain([
                    SlashCommand::Thinking(false),
                    SlashCommand::Effort(Effort::High),
                ])
                .collect(),
            message: Some(Prompt::from("go")),
        };
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
            ])
            .collect();
        assert_eq!(batch_commands(&chat, &batch), expected);
    }
}
