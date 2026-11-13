use krad_chat::start_chat;
use krad_domain::{AttentionTarget, GatewayAccess, Sight};
use krad_protocol::{AgentRef, AttentionCommand, ChatCommand, ModelsCommand, Reply, UsageCommand};

use super::{answer_to, Runtime};
use crate::chats::{
    ChatCheckpoint, ChatEarlier, ChatEffect, ChatOpening, ChatStart, ChatWork, Chats, FilesIndexed,
};
use crate::input::{Input, LiveStart};

fn start_live_chat(start: LiveStart, gateway: Option<&GatewayAccess>) {
    let LiveStart {
        launch,
        session,
        sink,
        reply,
    } = start;
    let started = launch
        .resolve(gateway)
        .and_then(|backend| start_chat(backend, &session, sink).map_err(|error| error.to_string()));
    if reply.send(started).is_err() {
        tracing::trace!("a client left before its chat started");
    }
}

impl Runtime {
    fn with_chat_work<T>(&mut self, act: impl FnOnce(&mut Chats, &mut ChatWork<'_>) -> T) -> T {
        let mut effects = Vec::new();
        let mut work = ChatWork {
            jobs: &self.jobs,
            gateway: self.gateway.access.as_ref(),
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
            Input::MentionIndex { place, reply } => {
                self.chats.mentions.ask(place, reply, &self.jobs);
            }
            Input::ForgetMentionIndexes => self.chats.mentions.forget(),
            Input::StartLiveChat(start) => {
                start_live_chat(*start, self.gateway.access.as_ref());
            }
            Input::Gateway(input) => self.gateway.apply(input, &self.jobs),
            _ => {}
        }
    }

    fn open_chat(&mut self, chat: &AgentRef, start: ChatStart) {
        self.with_chat_work(|chats, work| chats.open(chat, start, work));
    }

    pub(super) fn execute_chat(&mut self, command: ChatCommand) -> Option<Reply> {
        self.with_chat_work(|chats, work| chats.execute(command, work))
    }

    pub(super) fn flush_chats(&mut self) {
        self.with_chat_work(Chats::flush_all);
    }

    pub(super) fn hear_chats(&mut self) {
        self.with_chat_work(Chats::hear);
    }

    pub(super) fn chat_opened(&mut self, opening: ChatOpening) {
        self.with_chat_work(|chats, work| chats.opened(opening, work));
    }

    pub(super) fn chat_earlier_read(&mut self, earlier: ChatEarlier) {
        self.with_chat_work(|chats, work| chats.earlier_read(earlier, work));
    }

    pub(super) fn chat_checkpoint_taken(&mut self, taken: ChatCheckpoint) {
        self.with_chat_work(|chats, work| chats.checkpoint_taken(taken, work));
    }

    pub(super) fn files_indexed(&mut self, indexed: FilesIndexed) {
        self.chats
            .mentions
            .indexed(indexed, std::time::Instant::now());
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
