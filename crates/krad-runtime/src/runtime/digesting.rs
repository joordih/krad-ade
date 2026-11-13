use krad_chat::{new_session_id, start_chat, LiveSession};
use krad_domain::{
    digest_decision, digest_prompt, facts_from_reply, history_of, ChatDigests, ChatPhase,
    DigestChat, DigestCheck, DigestDecision, DigestDraft, MemoryOwner, PermissionMode, Prompt,
};
use krad_protocol::AgentRef;
use std::time::Instant;

use super::Runtime;
use crate::chats::{ChatBrief, ChatRole, DigestSource};
use crate::kraddies::{chat_owner, reply_sink, DigestReply, DigestWriter, Watched};

const DIGEST_MODE: PermissionMode = PermissionMode::Plan;

impl Runtime {
    fn watched(&self, chat: &AgentRef) -> Option<Watched> {
        let data = self.environments.data(&chat.environment)?;
        let session = data.session_of(chat.terminal)?;
        let title = data.session(session)?.name.clone();
        Some(Watched {
            owner: chat_owner(Some(data), chat.terminal),
            chat: DigestChat {
                environment: chat.environment.clone(),
                session,
                terminal: chat.terminal,
                title,
                folder: data.group_of(chat.terminal).map(|group| group.root.clone()),
                resume: None,
            },
        })
    }

    fn digest_check(
        &self,
        source: &DigestSource,
        watched: &Watched,
        phase: ChatPhase,
    ) -> Option<DigestCheck> {
        let digests = match watched.owner.kraddie() {
            Some(id) => self.kraddies.state(id)?.kraddie.settings.digests,
            None => ChatDigests::Automatic,
        };
        let attempted = self
            .kraddies
            .digests
            .attempted
            .get(&source.chat)
            .copied()
            .unwrap_or_default();
        let covered = self
            .kraddies
            .book(&watched.owner)
            .and_then(|book| book.of_chat(&watched.chat))
            .map_or_default(|digest| digest.covered)
            .max(attempted);
        let spent = self
            .environments
            .data(&source.chat.environment)
            .map_or_default(|data| {
                data.group_spend(data.group_of_chat(source.chat.terminal))
                    .cost
            });
        Some(DigestCheck {
            phase,
            messages: history_of(&source.messages).len(),
            covered,
            digests,
            budget: self.settings.current.spending.status(spent),
            writing: self.kraddies.digests.writing.contains_key(&source.chat),
        })
    }

    pub(super) fn tick_digests(&mut self, now: Instant) {
        for overdue in self.kraddies.digests.overdue(now) {
            self.digest_heard(&overdue, DigestReply::Failed(String::new()));
        }
        let open: Vec<(AgentRef, bool)> = self
            .chats
            .open_chats()
            .map(|(chat, working)| (chat.clone(), working))
            .collect();
        let watch = &mut self.kraddies.digests;
        for (chat, working) in &open {
            if *working {
                watch.quiet.remove(chat);
            } else {
                watch.quiet.entry(chat.clone()).or_insert(now);
            }
        }
        let is_open = |chat: &AgentRef| open.iter().any(|(open, _)| open == chat);
        watch.quiet.retain(|chat, _| is_open(chat));
        watch.attempted.retain(|chat, _| is_open(chat));
        watch.known.retain(|chat, _| is_open(chat));
        if !watch.check_due(now) {
            return;
        }
        for (chat, _) in open {
            if let Some(watched) = self.watched(&chat) {
                self.kraddies.digests.known.insert(chat.clone(), watched);
            }
            let phase = match self.kraddies.digests.quiet.get(&chat) {
                Some(since) => ChatPhase::Quiet { since: *since },
                None => ChatPhase::Working,
            };
            if let Some(source) = self.chats.digest_source(&chat) {
                self.consider_digest(source, phase, now);
            }
        }
    }

    pub(super) fn digest_ended_chats(&mut self) {
        for source in self.chats.take_ended() {
            self.kraddies.digests.quiet.remove(&source.chat);
            self.consider_digest(source, ChatPhase::Ended, Instant::now());
        }
    }

    fn consider_digest(&mut self, source: DigestSource, phase: ChatPhase, now: Instant) {
        let watched = self
            .kraddies
            .digests
            .known
            .get(&source.chat)
            .cloned()
            .or_else(|| self.watched(&source.chat));
        let Some(watched) = watched else {
            return;
        };
        let Some(check) = self.digest_check(&source, &watched, phase) else {
            return;
        };
        if let DigestDecision::Write(reason) = digest_decision(&check, now) {
            tracing::debug!(
                env = source.chat.environment.as_str(),
                chat = source.chat.terminal.as_u64(),
                ?reason,
                "writing a chat digest"
            );
            self.write_digest(source, watched);
        }
    }

    fn write_digest(&mut self, source: DigestSource, watched: Watched) {
        let history = history_of(&source.messages);
        let covered = history.len();
        self.kraddies
            .digests
            .attempted
            .insert(source.chat.clone(), covered);
        let brief = ChatBrief {
            model: source.model.clone(),
            effort: source.effort,
            permission_mode: Some(DIGEST_MODE),
            mcp_servers: None,
            role: ChatRole::Digest,
        };
        let backend = self.with_chat_work(|_, work| {
            crate::chats::resolve_once((&source.chat, source.harness), &brief, work)
        });
        let sink = reply_sink(source.chat.clone(), self.kraddies.inbox.clone());
        let started = backend.map_err(String::from).and_then(|backend| {
            start_chat(backend, &LiveSession::Start(new_session_id()), sink)
                .map_err(|error| error.to_string())
        });
        let live = match started {
            Ok(live) => live,
            Err(error) => {
                tracing::warn!(%error, "the chat digest writer did not start");
                return;
            }
        };
        live.send(Prompt::from(digest_prompt(&watched.chat.title, &history)));
        self.kraddies.digests.writing.insert(
            source.chat,
            DigestWriter {
                watched,
                covered,
                started: Instant::now(),
                live: Some(live),
            },
        );
    }

    pub(super) fn digest_heard(&mut self, chat: &AgentRef, reply: DigestReply) {
        let Some(writer) = self.kraddies.digests.writing.remove(chat) else {
            return;
        };
        if let Some(live) = &writer.live {
            live.stop();
        }
        let text = match reply {
            DigestReply::Text(text) => text,
            DigestReply::Failed(reason) => {
                tracing::debug!(env = chat.environment.as_str(), chat = chat.terminal.as_u64(), %reason, "no chat digest");
                return;
            }
        };
        let draft = DigestDraft {
            chat: writer.watched.chat,
            facts: facts_from_reply(&text),
            covered: writer.covered,
            written: self.kraddies.now().at,
        };
        let owner: MemoryOwner = writer.watched.owner;
        self.kraddies.record_digest(&owner, draft);
    }
}
