use futures::channel::oneshot;
use futures::future::{select, Either};
use gpui_kit::Context;
use krad_chat::{
    new_session_id, start_chat, BusVars, ChatBackend, ChatEvent, LiveEvent, LiveSession, Speaker,
};
use krad_domain::{
    digest_decision, digest_prompt, facts_from_reply, AgentPreset, ChatDigests, ChatPhase,
    DigestChat, DigestCheck, DigestDecision, DigestDraft, EnvironmentName, HistoryMessage,
    HistorySpeaker, MemoryOwner, PermissionMode, Prompt, TerminalId,
};
use std::time::{Duration, Instant, SystemTime};

use super::super::chat::{AnswerPart, ChatMessage};
use super::super::EnvironmentShell;
use super::ChatKey;

pub(super) const DIGEST_CHECK_EVERY: Duration = Duration::from_mins(1);
const DIGEST_TIMEOUT: Duration = Duration::from_mins(4);
const DIGEST_MODE: PermissionMode = PermissionMode::Plan;
const BLOCK_SEPARATOR: &str = "\n\n";
const WRITER_STOPPED: &str = "the writer stopped";
const WRITER_TOO_SLOW: &str = "the writer took too long";

struct DigestJob {
    owner: MemoryOwner,
    chat: DigestChat,
    harness: AgentPreset,
    messages: Vec<HistoryMessage>,
}

enum DigestReply {
    Text(String),
    Failed(String),
}

#[must_use]
pub(super) fn history_of(messages: &[ChatMessage]) -> Vec<HistoryMessage> {
    messages
        .iter()
        .filter_map(|message| {
            let (speaker, text) = match message.speaker {
                Speaker::User => (HistorySpeaker::User, message.text.clone()),
                Speaker::Assistant => {
                    let text = message
                        .parts
                        .iter()
                        .filter_map(|part| match part {
                            AnswerPart::Text(text) => Some(text.as_str()),
                            AnswerPart::Thinking(_) | AnswerPart::Tool(_) | AnswerPart::Ask(_) => {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(BLOCK_SEPARATOR);
                    (HistorySpeaker::Agent, text)
                }
            };
            (!text.trim().is_empty()).then_some(HistoryMessage { speaker, text })
        })
        .collect()
}

fn quiet_backend(backend: ChatBackend) -> Option<ChatBackend> {
    match backend {
        ChatBackend::Claude(mut config) => {
            config.system_prompt = None;
            config.bus_vars = BusVars(Vec::new());
            config.permission_mode = Some(DIGEST_MODE);
            Some(ChatBackend::Claude(config))
        }
        ChatBackend::Codex(mut config) => {
            config.system_prompt = None;
            config.bus_vars = BusVars(Vec::new());
            config.permission_mode = Some(DIGEST_MODE);
            Some(ChatBackend::Codex(config))
        }
        ChatBackend::Opencode(server, mut config) => {
            config.system_prompt = None;
            config.bus_vars = BusVars(Vec::new());
            config.permission_mode = Some(DIGEST_MODE);
            Some(ChatBackend::Opencode(server, config))
        }
        ChatBackend::Gateway(mut config) => {
            config.system_prompt = None;
            config.bus_vars = BusVars(Vec::new());
            config.permission_mode = Some(DIGEST_MODE);
            config.mcp = Vec::new();
            Some(ChatBackend::Gateway(config))
        }
        ChatBackend::Acp(_) => None,
    }
}

fn reply_sink(reply: oneshot::Sender<DigestReply>) -> impl FnMut(LiveEvent) + Send + 'static {
    let mut reply = Some(reply);
    let mut whole = String::new();
    let mut pieces = String::new();
    move |event| {
        let finished = match event {
            LiveEvent::Chat(ChatEvent::Text(text)) => {
                whole.push_str(&text);
                whole.push('\n');
                None
            }
            LiveEvent::Chat(ChatEvent::TextDelta(text)) => {
                pieces.push_str(&text);
                None
            }
            LiveEvent::Chat(ChatEvent::Done) => {
                let text = if whole.trim().is_empty() {
                    &mut pieces
                } else {
                    &mut whole
                };
                Some(DigestReply::Text(std::mem::take(text)))
            }
            LiveEvent::Chat(ChatEvent::Error(error)) => Some(DigestReply::Failed(error)),
            _ => None,
        };
        let Some(finished) = finished else {
            return;
        };
        if let Some(sender) = reply.take() {
            sender.send(finished).unwrap_or_default();
        }
    }
}

impl EnvironmentShell {
    fn digest_check(&self, key: &ChatKey, phase: ChatPhase) -> Option<DigestCheck> {
        let (env, chat) = key;
        let live = self.envs.get(env)?;
        let surface = live.chats.get(chat)?;
        surface.structured()?;
        let owner = self.chat_owner(env, *chat);
        if !self.kraddies.memory.loaded.contains(&owner) {
            return None;
        }
        let digests = match owner.kraddie() {
            Some(id) => self.kraddies.entry(id)?.kraddie.settings.digests,
            None => ChatDigests::Automatic,
        };
        let attempted = self
            .kraddies
            .memory
            .attempted
            .get(key)
            .copied()
            .unwrap_or_default();
        let covered = self
            .digest_chat(env, *chat)
            .and_then(|found| {
                self.kraddies
                    .memory
                    .book(&owner)?
                    .of_chat(&found)
                    .map(|digest| digest.covered)
            })
            .unwrap_or_default()
            .max(attempted);
        let group = live.data.group_of_chat(*chat);
        Some(DigestCheck {
            phase,
            messages: history_of(&surface.messages).len(),
            covered,
            digests,
            budget: self
                .settings
                .spending
                .status(live.data.group_spend(group).cost),
            writing: self.kraddies.memory.writing.contains_key(key),
        })
    }

    fn digest_chat(&self, env: &EnvironmentName, chat: TerminalId) -> Option<DigestChat> {
        let live = self.envs.get(env)?;
        let session = live.data.session_of(chat)?;
        let title = live.data.session(session)?.name.clone();
        Some(DigestChat {
            environment: env.clone(),
            session,
            terminal: chat,
            title,
            folder: live.data.group_of(chat).map(|group| group.root.clone()),
            resume: None,
        })
    }

    pub(super) fn check_digests(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        let chats: Vec<ChatKey> = self
            .envs
            .iter()
            .flat_map(|(env, live)| {
                live.chats
                    .iter()
                    .filter(|(_, surface)| surface.structured().is_some())
                    .map(move |(chat, surface)| {
                        ((env.clone(), *chat), surface.working_since.get().is_some())
                    })
            })
            .map(|(key, working)| {
                if working {
                    self.kraddies.memory.quiet.remove(&key);
                } else {
                    self.kraddies.memory.quiet.entry(key.clone()).or_insert(now);
                }
                key
            })
            .collect();
        self.kraddies
            .memory
            .quiet
            .retain(|key, _| chats.contains(key));
        self.kraddies
            .memory
            .attempted
            .retain(|key, _| chats.contains(key));
        for key in chats {
            let phase = match self.kraddies.memory.quiet.get(&key) {
                Some(since) => ChatPhase::Quiet { since: *since },
                None => ChatPhase::Working,
            };
            let Some(check) = self.digest_check(&key, phase) else {
                continue;
            };
            if let DigestDecision::Write(reason) = digest_decision(&check, now) {
                tracing::debug!(
                    env = key.0.as_str(),
                    chat = key.1.as_u64(),
                    ?reason,
                    "writing a chat digest"
                );
                self.write_digest(&key, cx);
            }
        }
    }

    pub(in crate::environment_shell) fn digest_ended_chat(
        &mut self,
        env: &EnvironmentName,
        chat: TerminalId,
        cx: &mut Context<Self>,
    ) {
        let key = (env.clone(), chat);
        let Some(check) = self.digest_check(&key, ChatPhase::Ended) else {
            return;
        };
        if matches!(
            digest_decision(&check, Instant::now()),
            DigestDecision::Write(_)
        ) {
            self.write_digest(&key, cx);
        }
    }

    fn digest_job(&self, key: &ChatKey) -> Option<DigestJob> {
        let (env, chat) = key;
        let surface = self.envs.get(env)?.chats.get(chat)?;
        Some(DigestJob {
            owner: self.chat_owner(env, *chat),
            chat: self.digest_chat(env, *chat)?,
            harness: surface.harness(),
            messages: history_of(&surface.messages),
        })
    }

    fn write_digest(&mut self, key: &ChatKey, cx: &mut Context<Self>) {
        let Some(job) = self.digest_job(key) else {
            return;
        };
        self.kraddies
            .memory
            .attempted
            .insert(key.clone(), job.messages.len());
        let backend = match self.chat_backend(&key.0, key.1, job.harness, cx) {
            Ok(backend) => quiet_backend(backend),
            Err(error) => {
                tracing::debug!(%error, "no harness free to write a chat digest");
                None
            }
        };
        let Some(backend) = backend else {
            return;
        };
        let (sender, receiver) = oneshot::channel();
        let session = LiveSession::Start(new_session_id());
        let live = match start_chat(backend, &session, reply_sink(sender)) {
            Ok(live) => live,
            Err(error) => {
                tracing::warn!(%error, "the chat digest writer did not start");
                return;
            }
        };
        live.send(Prompt::from(digest_prompt(&job.chat.title, &job.messages)));
        self.kraddies.memory.writing.insert(key.clone(), live);
        let key = key.clone();
        let timer = cx.background_executor().timer(DIGEST_TIMEOUT);
        cx.spawn(async move |this, cx| {
            let reply = match select(receiver, timer).await {
                Either::Left((Ok(reply), _)) => reply,
                Either::Left((Err(_), _)) => DigestReply::Failed(WRITER_STOPPED.to_owned()),
                Either::Right(_) => DigestReply::Failed(WRITER_TOO_SLOW.to_owned()),
            };
            this.update(cx, |shell, cx| shell.digest_written(&key, job, reply, cx))
                .unwrap_or_default();
        })
        .detach();
    }

    fn digest_written(
        &mut self,
        key: &ChatKey,
        job: DigestJob,
        reply: DigestReply,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.kraddies.memory.writing.remove(key) {
            live.stop();
        }
        let text = match reply {
            DigestReply::Text(text) => text,
            DigestReply::Failed(reason) => {
                tracing::debug!(env = key.0.as_str(), chat = key.1.as_u64(), %reason, "no chat digest");
                return;
            }
        };
        let draft = DigestDraft {
            chat: job.chat,
            facts: facts_from_reply(&text),
            covered: job.messages.len(),
            written: SystemTime::now(),
        };
        if self
            .kraddies
            .memory
            .book_mut(&job.owner)
            .record(draft)
            .is_some()
        {
            tracing::info!(owner = %job.owner, "chat digest kept");
            self.save_digests(&job.owner, cx);
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn message(speaker: Speaker, text: &str, parts: Vec<AnswerPart>) -> ChatMessage {
        ChatMessage {
            speaker,
            text: text.to_owned(),
            parts,
            error: None,
            running: false,
            mentions: Vec::new(),
            checklist: None,
            rewind: None,
            tool_outputs: HashMap::new(),
        }
    }

    #[test]
    fn the_digest_reads_what_was_said_not_tools_or_thoughts() {
        let messages = [
            message(Speaker::User, "Which port?", Vec::new()),
            message(
                Speaker::Assistant,
                "",
                vec![
                    AnswerPart::Thinking("hmm".to_owned()),
                    AnswerPart::Text("Port 8080.".to_owned()),
                    AnswerPart::Text("Set in config.toml.".to_owned()),
                ],
            ),
            message(
                Speaker::Assistant,
                "",
                vec![AnswerPart::Thinking("only".to_owned())],
            ),
        ];
        let history = history_of(&messages);
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].speaker, HistorySpeaker::User);
        assert_eq!(history[1].text, "Port 8080.\n\nSet in config.toml.");
    }

    #[test]
    fn the_writer_keeps_the_whole_answer_or_its_pieces() {
        let (sender, mut receiver) = oneshot::channel();
        let mut sink = reply_sink(sender);
        sink(LiveEvent::Chat(ChatEvent::TextDelta("- a".to_owned())));
        sink(LiveEvent::Chat(ChatEvent::TextDelta("\n- b".to_owned())));
        sink(LiveEvent::Chat(ChatEvent::Done));
        sink(LiveEvent::Chat(ChatEvent::Done));
        assert!(matches!(
            receiver.try_recv(),
            Ok(Some(DigestReply::Text(text))) if text == "- a\n- b"
        ));
    }
}
