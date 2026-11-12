mod actions;
mod source;

pub(super) use actions::ChatHitAction;

use gpui_kit::{AppContext as _, Context, Task};
use krad_domain::{
    search_chats, AgentPreset, Archive, ArchivedSession, ChatDocument, ChatHit, EnvironmentName,
    GroupId, HistoryQuery, ResumeId, TerminalId,
};
use krad_store::{acp_transcript_file, gateway_sessions_dir};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::EnvironmentShell;
use source::{merged, HistorySource, Indexed};

const HITS_FOUND: usize = 12;

type ChatKey = (EnvironmentName, TerminalId);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Revision {
    messages: usize,
    last_text: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ChatPlace {
    Open,
    Archived,
}

#[derive(Clone, Debug)]
pub(super) struct GroupChat {
    pub(super) id: TerminalId,
    pub(super) title: String,
    pub(super) harness: AgentPreset,
    pub(super) resume: ResumeId,
    pub(super) place: ChatPlace,
}

fn archived_chats(archive: &Archive, group: GroupId) -> Vec<GroupChat> {
    let in_sessions = archive
        .sessions_of(group)
        .into_iter()
        .flat_map(ArchivedSession::all_chats);
    let loose = archive.chats_of(group).into_iter().map(|loose| &loose.chat);
    loose
        .chain(in_sessions)
        .map(|chat| GroupChat {
            id: chat.terminal,
            title: chat.title.clone(),
            harness: chat.harness,
            resume: chat.resume.clone(),
            place: ChatPlace::Archived,
        })
        .collect()
}

#[derive(Default)]
pub(super) struct ChatSearch {
    indexed: HashMap<ChatKey, Indexed>,
    revisions: HashMap<ChatKey, Revision>,
    reading: HashSet<ChatKey>,
    query: Option<HistoryQuery>,
    hits: Vec<ChatHit>,
    task: Option<Task<()>>,
}

impl EnvironmentShell {
    pub(super) fn group_chats(&self) -> Vec<GroupChat> {
        let Some(group) = self.active_group().map(|group| group.id) else {
            return Vec::new();
        };
        let Some(live) = self.envs.get(&self.active) else {
            return Vec::new();
        };
        let mut chats: Vec<GroupChat> = live
            .data
            .nodes
            .iter()
            .rev()
            .filter(|node| node.group == Some(group))
            .filter_map(|node| {
                Some(GroupChat {
                    id: node.id,
                    title: node.title.clone(),
                    harness: node.harness.filter(|preset| preset.is_agent())?,
                    resume: node.resume.clone()?,
                    place: ChatPlace::Open,
                })
            })
            .collect();
        chats.extend(archived_chats(&live.data.archive, group));
        chats
    }

    pub(super) fn indexed_chat(&self, id: TerminalId) -> Option<Arc<ChatDocument>> {
        self.chat_search
            .indexed
            .get(&(self.active.clone(), id))?
            .document
            .clone()
    }

    fn chat_revision(&self, env: &EnvironmentName, id: TerminalId) -> Option<Revision> {
        let chat = self.envs.get(env)?.chats.get(&id)?;
        Some(Revision {
            messages: chat.messages.len(),
            last_text: chat.messages.last().map_or(0, |message| message.text.len()),
        })
    }

    fn history_source(&self, env: &EnvironmentName, chat: &GroupChat) -> Option<HistorySource> {
        let resume = chat.resume.clone();
        let root = self.active_group().map(|group| group.root.clone());
        let link = self.env_link(env);
        match chat.harness {
            AgentPreset::ClaudeCode => Some(HistorySource::Claude {
                link,
                resume,
                root,
                cursor: self
                    .chat_search
                    .indexed
                    .get(&(env.clone(), chat.id))
                    .map_or_else(Default::default, |indexed| indexed.cursor.clone()),
            }),
            AgentPreset::Gateway => Some(HistorySource::Gateway {
                sessions: gateway_sessions_dir()?,
                cwd: root.unwrap_or_else(|| self.chat_folder(env, chat.id)),
                resume,
            }),
            AgentPreset::Acp(_) => Some(HistorySource::Acp {
                file: acp_transcript_file(env, chat.id)?,
            }),
            AgentPreset::Codex => {
                let (runner, cwd) = self
                    .harness_runner(&link, AgentPreset::Codex, root.as_deref())
                    .ok()?;
                Some(HistorySource::Codex {
                    runner,
                    cwd,
                    resume,
                })
            }
            AgentPreset::Opencode => Some(HistorySource::Opencode {
                server: self.opencode_server_of(env)?.clone(),
                resume,
            }),
            AgentPreset::Shell => None,
        }
    }

    fn needs_reading(&self, key: &ChatKey, place: ChatPlace) -> bool {
        if self.chat_search.reading.contains(key) {
            return false;
        }
        if place == ChatPlace::Archived {
            return !self.chat_search.indexed.contains_key(key);
        }
        let known = self.chat_search.revisions.get(key);
        match self.chat_revision(&key.0, key.1) {
            Some(revision) => known != Some(&revision),
            None => true,
        }
    }

    pub(super) fn refresh_chat_index(&mut self, cx: &mut Context<Self>) {
        let env = self.active.clone();
        for chat in self.group_chats() {
            let key = (env.clone(), chat.id);
            if !self.needs_reading(&key, chat.place) {
                continue;
            }
            let Some(source) = self.history_source(&env, &chat) else {
                continue;
            };
            self.read_chat_history(key, chat, source, cx);
        }
    }

    fn read_chat_history(
        &mut self,
        key: ChatKey,
        chat: GroupChat,
        source: HistorySource,
        cx: &mut Context<Self>,
    ) {
        let revision = self.chat_revision(&key.0, key.1);
        let indexed = self
            .chat_search
            .indexed
            .get(&key)
            .cloned()
            .unwrap_or_default();
        self.chat_search.reading.insert(key.clone());
        let GroupChat { id, title, .. } = chat;
        let read = cx.background_spawn(async move {
            source.read().map(|read| merged(indexed, id, title, read))
        });
        cx.spawn(async move |this, cx| {
            let read = read.await;
            this.update(cx, |shell, cx| {
                shell.chat_search.reading.remove(&key);
                match read {
                    Ok(indexed) => {
                        if let Some(revision) = revision {
                            shell.chat_search.revisions.insert(key.clone(), revision);
                        }
                        shell.chat_search.indexed.insert(key, indexed);
                        shell.run_chat_search(cx);
                    }
                    Err(error) => {
                        tracing::debug!(
                            chat = id.as_u64(),
                            error = %error,
                            "chat history could not be read for search"
                        );
                    }
                }
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn search_chat_history(&mut self, query: &str, cx: &mut Context<Self>) {
        let parsed = HistoryQuery::parse(query);
        if parsed == self.chat_search.query {
            return;
        }
        self.chat_search.query = parsed;
        self.run_chat_search(cx);
    }

    fn run_chat_search(&mut self, cx: &mut Context<Self>) {
        let Some(query) = self.chat_search.query.clone() else {
            self.chat_search.hits.clear();
            self.chat_search.task = None;
            cx.notify();
            return;
        };
        let documents: Vec<Arc<ChatDocument>> = self
            .group_chats()
            .iter()
            .filter_map(|chat| self.indexed_chat(chat.id))
            .collect();
        let search = cx.background_spawn(async move {
            search_chats(documents.iter().map(AsRef::as_ref), &query, HITS_FOUND)
        });
        self.chat_search.task = Some(cx.spawn(async move |this, cx| {
            let hits = search.await;
            this.update(cx, |shell, cx| {
                shell.chat_search.hits = hits;
                cx.notify();
            })
            .unwrap_or_default();
        }));
    }

    pub(super) fn palette_chat_hits(&self) -> &[ChatHit] {
        if self.chat_search.query.is_none() {
            return &[];
        }
        &self.chat_search.hits
    }
}
