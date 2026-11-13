use krad_domain::{AttentionTarget, Sight};
use krad_protocol::{AgentRef, AttentionCommand, ChatCommand, ModelsCommand, Reply, UsageCommand};

use super::{answer_to, Runtime};
use crate::chats::{ChatEffect, ChatOpening, ChatStart, ChatWork, Chats};
use crate::input::Input;

impl Runtime {
    fn with_chat_work<T>(&mut self, act: impl FnOnce(&mut Chats, &mut ChatWork<'_>) -> T) -> T {
        let mut effects = Vec::new();
        let mut work = ChatWork {
            jobs: &self.jobs,
            outbox: &mut self.outbox,
            effects: &mut effects,
        };
        let done = act(&mut self.chats, &mut work);
        for effect in effects {
            self.chat_effect(effect);
        }
        done
    }

    pub(super) fn apply_chat_input(&mut self, input: Input) {
        match input {
            Input::OpenChat { chat, start } => self.open_chat(&chat, *start),
            Input::CloseChat(chat) => self.chats.close(&chat, &mut self.outbox),
            Input::ChatPage {
                chat,
                before,
                count,
                reply,
            } => answer_to(reply, self.chats.page(&chat, before, count)),
            _ => {}
        }
    }

    fn open_chat(&mut self, chat: &AgentRef, start: ChatStart) {
        self.with_chat_work(|chats, work| chats.open(chat, start, work));
    }

    pub(super) fn execute_chat(&mut self, command: ChatCommand) -> Option<Reply> {
        self.with_chat_work(|chats, work| chats.execute(command, work))
    }

    pub(super) fn hear_chats(&mut self) {
        self.with_chat_work(Chats::hear);
    }

    pub(super) fn chat_opened(&mut self, opening: ChatOpening) {
        self.with_chat_work(|chats, work| chats.opened(opening, work));
    }

    fn chat_effect(&mut self, effect: ChatEffect) {
        let reply = match effect {
            ChatEffect::Raise { chat, kind } => {
                let title = self.environments.node_title(&chat).unwrap_or_default();
                let command = AttentionCommand::Raise {
                    target: AttentionTarget::new(chat.environment, chat.terminal),
                    kind,
                    sight: Sight::Unwatched,
                    title,
                };
                self.attention.execute(&command, &mut self.outbox)
            }
            ChatEffect::Usage {
                host,
                harness,
                news,
            } => {
                let command = UsageCommand::Heard {
                    host,
                    harness,
                    news,
                };
                self.usage.execute(&command, &self.jobs, &mut self.outbox)
            }
            ChatEffect::Models {
                host,
                harness,
                models,
            } => {
                let command = ModelsCommand::Keep {
                    host,
                    harness,
                    models,
                };
                self.models.execute(&command, &mut self.outbox)
            }
        };
        if let Some(Reply::Refused { code, .. }) = reply {
            tracing::debug!(?code, "a chat effect was refused");
        }
    }
}
