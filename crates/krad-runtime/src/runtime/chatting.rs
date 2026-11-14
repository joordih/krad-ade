use krad_domain::EnvironmentName;
use krad_domain::{AgentPreset, AttentionTarget, Host, ModelOffer, Sight};
use krad_protocol::{
    AgentRef, Answer, AttentionCommand, ChatCommand, ErrorCode, EventKind, ModelsCommand, Reply,
    RequestId, UsageCommand,
};
use std::path::PathBuf;

use super::{refused, Runtime};
use crate::chats::{
    ChatCheckpoint, ChatEarlier, ChatEffect, ChatOpening, ChatStart, ChatWork, Chats, FilesIndexed,
    LaunchFacts,
};
use crate::chats::{MentionQuery, UNKNOWN_CHAT};
use crate::environments::UNKNOWN_ENVIRONMENT;
use crate::input::Input;
use crate::opencode::{OpencodeListed, OpencodeStarted};

impl Runtime {
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
            Input::OfferBuffers {
                environment,
                buffers,
            } => self.buffers.offer(environment, buffers),
            Input::Opencode(input) => self.opencode.apply(input, &self.jobs, &mut self.outbox),
            _ => {}
        }
    }

    pub(super) fn open_chat(&mut self, chat: &AgentRef, start: ChatStart) {
        self.with_chat_work(|chats, work| chats.open(chat, start, work));
    }

    pub(super) fn execute_chat(&mut self, id: RequestId, command: ChatCommand) -> Option<Reply> {
        match command {
            ChatCommand::Open { chat, start } => {
                self.open_chat(&chat, ChatStart::from(*start));
                Some(Reply::Accepted)
            }
            ChatCommand::Close { chat } => {
                self.chats.close(&chat, &mut self.outbox);
                self.cloud_runs.forget(&chat);
                Some(Reply::Accepted)
            }
            ChatCommand::Page {
                chat,
                before,
                count,
            } => Some(self.answer_chat_page(id, &chat, (before, count))),
            ChatCommand::Mentions {
                environment,
                root,
                query,
                limit,
            } => Some(self.ask_mentions(
                &environment,
                root,
                MentionQuery {
                    request: id,
                    query,
                    limit,
                },
            )),
            ChatCommand::Transcript { environment, ask } => {
                Some(self.ask_transcript(id, &environment, ask))
            }
            ChatCommand::Subagents { environment, ask } => {
                Some(self.ask_subagents(id, &environment, ask))
            }
            ChatCommand::History { environment, ask } => {
                Some(self.ask_history(id, &environment, ask))
            }
            ChatCommand::ForgetMentions => {
                self.chats.mentions.forget();
                Some(Reply::Accepted)
            }
            ChatCommand::AttachImage { chat, image } => Some(self.attach_chat_image(chat, image)),
            ChatCommand::ForgetImages { chat } => Some(self.forget_chat_images(chat)),
            ChatCommand::ForgetTranscript { chat } => {
                self.chats.forget_transcript(&chat);
                Some(Reply::Accepted)
            }
            ChatCommand::ReadImage { image } => Some(self.read_chat_image(id, image)),
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
            .indexed(indexed, std::time::Instant::now(), &mut self.outbox);
    }

    fn answer_chat_page(
        &mut self,
        request: RequestId,
        chat: &AgentRef,
        (before, count): (usize, usize),
    ) -> Reply {
        let Some(page) = self.chats.page(chat, before, count) else {
            return refused(ErrorCode::NotFound, UNKNOWN_CHAT);
        };
        self.outbox.push(EventKind::Answered {
            request,
            answer: Box::new(Answer::ChatPage(page)),
        });
        Reply::Accepted
    }

    fn ask_mentions(
        &mut self,
        environment: &EnvironmentName,
        root: PathBuf,
        query: MentionQuery,
    ) -> Reply {
        let Some(host) = self.environments.host_of(environment) else {
            return refused(ErrorCode::NotFound, UNKNOWN_ENVIRONMENT);
        };
        let link = self.hosts.link(&host);
        self.chats
            .mentions
            .ask((link, root), query, (&self.jobs, &mut self.outbox));
        Reply::Accepted
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
