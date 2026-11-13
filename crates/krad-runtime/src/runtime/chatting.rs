use krad_chat::start_chat;
use krad_domain::{AgentPreset, AttentionTarget, Host, ModelOffer, Sight};
use krad_protocol::{
    AgentRef, AttentionCommand, ChatCommand, EventKind, ModelsCommand, Reply, UsageCommand,
};

use super::{answer_to, Runtime};
use crate::chats::{
    ChatCheckpoint, ChatEarlier, ChatEffect, ChatOpening, ChatStart, ChatWork, Chats, FilesIndexed,
    LaunchFacts,
};
use crate::input::{Input, LiveStart};
use crate::opencode::{OpencodeListed, OpencodeStarted};

impl Runtime {
    fn start_live_chat(&mut self, start: LiveStart) {
        let LiveStart {
            chat,
            harness,
            brief,
            session,
            sink,
            reply,
        } = start;
        let started = self
            .with_chat_work(|_, work| crate::chats::resolve_once((&chat, harness), &brief, work));
        let started = started.map_err(String::from).and_then(|backend| {
            start_chat(backend, &session, sink).map_err(|error| error.to_string())
        });
        if reply.send(started).is_err() {
            tracing::trace!("a client left before its chat started");
        }
    }

    pub(super) fn with_chat_work<T>(
        &mut self,
        act: impl FnOnce(&mut Chats, &mut ChatWork<'_>) -> T,
    ) -> T {
        let mut effects = Vec::new();
        let mut work = ChatWork {
            jobs: &self.jobs,
            cloud: &mut self.cloud_runs,
            adapters: &mut self.adapters,
            gateway: self.gateway.reached(&self.settings.current.gateway),
            opencode: &mut self.opencode,
            outbox: &mut self.outbox,
            effects: &mut effects,
            facts: LaunchFacts {
                settings: &self.settings.current,
                environments: &self.environments,
                hosts: &self.hosts,
                models: &self.models,
                bus: self.bus.as_ref().map(crate::bus::Bus::access),
                usage_budget: self.usage.budget(),
                kraddies: &self.kraddies,
                buffers: &self.buffers,
                secrets: &self.secrets,
                runners: &self.runners,
            },
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
            Input::CloseChat(chat) => {
                self.chats.close(&chat, &mut self.outbox);
                self.cloud_runs.forget(&chat);
            }
            Input::ChatPage {
                chat,
                before,
                count,
                reply,
            } => answer_to(reply, self.chats.page(&chat, before, count)),
            Input::MentionIndex { place, reply } => {
                self.chats.mentions.ask(place, reply, &self.jobs);
            }
            Input::OfferBuffers {
                environment,
                buffers,
            } => self.buffers.offer(environment, buffers),
            Input::ForgetMentionIndexes => self.chats.mentions.forget(),
            Input::StartLiveChat(start) => self.start_live_chat(*start),
            Input::Opencode(input) => self.opencode.apply(input, &self.jobs, &mut self.outbox),
            _ => {}
        }
    }

    pub(super) fn open_chat(&mut self, chat: &AgentRef, start: ChatStart) {
        self.with_chat_work(|chats, work| chats.open(chat, start, work));
    }

    pub(super) fn execute_chat(&mut self, command: ChatCommand) -> Option<Reply> {
        match command {
            ChatCommand::AttachImage { chat, image } => Some(self.attach_chat_image(chat, image)),
            ChatCommand::ForgetImages { chat } => Some(self.forget_chat_images(chat)),
            command => self.with_chat_work(|chats, work| chats.execute(command, work)),
        }
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

    pub(super) fn acp_secrets_read(&mut self, read: crate::chats::AcpSecretsRead) {
        self.with_chat_work(|chats, work| chats.acp_secrets_read(read, work));
    }

    pub(super) fn opencode_started(&mut self, started: OpencodeStarted) {
        let Some(heard) = self.opencode.started(started, &mut self.outbox) else {
            return;
        };
        self.keep_opencode_models(heard.host.clone(), heard.models);
        self.with_chat_work(|chats, work| chats.opencode_ready(&heard.host, work));
    }

    pub(super) fn opencode_listed(&mut self, listed: OpencodeListed) {
        let Some(host) = self.opencode.listed(&listed, &self.jobs) else {
            return;
        };
        match listed.listed {
            Ok(models) => self.keep_opencode_models(host, models),
            Err(error) => self.outbox.push(EventKind::ModelListFailed {
                host,
                harness: AgentPreset::Opencode,
                error,
            }),
        }
    }

    fn keep_opencode_models(&mut self, host: Host, models: Vec<ModelOffer>) {
        self.chat_effect(ChatEffect::Models {
            host,
            harness: AgentPreset::Opencode,
            models,
        });
    }

    fn chat_effect(&mut self, effect: ChatEffect) {
        let reply = match effect {
            ChatEffect::McpTrust { folder, trust } => {
                self.settings
                    .trust_mcp_project(&folder, trust, &mut self.outbox);
                return;
            }
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
