use gpui_kit::{AppContext as _, Context, Task, Window};
use krad_chat::{
    new_session_id, read_claude_transcript_before, read_codex_transcript, read_engine_transcript,
    read_opencode_transcript, LiveSession,
};
use krad_domain::{
    AgentPreset, ChatChoice, ChatEntry, EnvironmentName, NodeKind, ResumeId, TerminalId,
    TerminalNode, DEFAULT_CHAT_HARNESS,
};
use krad_store::{acp_transcript_file, gateway_sessions_dir, read_acp_transcript};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

use super::super::super::chat::{fold_entry, last_permission_mode};
use super::super::super::opencode::OpencodeLink;
use super::super::super::EnvironmentShell;
use super::super::transcript::{fold_history, started_at, TailJob, TailRead};
use super::super::{Backend, Origin, SurfaceChat};
use super::{structured_chat, structured_mut, StructuredChat};

pub(in crate::environment_shell::surface) struct Earlier {
    file: PathBuf,
    end: u64,
}

impl Earlier {
    fn of(read: &TailRead) -> Option<Self> {
        let file = read.file.clone().filter(|_| read.skips_history())?;
        Some(Self {
            file,
            end: read.start,
        })
    }
}

fn prepend_history(chat: &mut SurfaceChat, entries: &[ChatEntry], started: Option<SystemTime>) {
    let mut messages = Vec::new();
    for entry in entries {
        fold_entry(&mut messages, entry);
    }
    if let Some(last) = messages.last_mut() {
        last.running = false;
    }
    messages.append(&mut chat.messages);
    chat.messages = messages;
    chat.origin = Origin::of(entries, started);
    chat.opened_thoughts.clear();
    chat.editing = None;
    chat.thread.remeasure();
}

pub(in crate::environment_shell) fn restored_chats(
    nodes: &[TerminalNode],
) -> HashMap<TerminalId, LiveSession> {
    nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Chat)
        .map(|node| {
            let session = node
                .resume
                .clone()
                .map_or_else(|| LiveSession::Start(new_session_id()), LiveSession::Resume);
            (node.id, session)
        })
        .collect()
}

const CONNECTING_TEXT: &str = "Connecting to";

const fn reads_history_on_host(harness: AgentPreset) -> bool {
    matches!(
        harness,
        AgentPreset::ClaudeCode | AgentPreset::Codex | AgentPreset::Opencode
    )
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn ensure_structured_chats(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let background: Vec<EnvironmentName> = self
            .envs
            .iter()
            .filter(|(_, live)| !live.pending_chats.is_empty())
            .map(|(name, _)| name.clone())
            .filter(|name| self.runs_in_background(name))
            .collect();
        let active = self.active.clone();
        for env in std::iter::once(active).chain(background) {
            self.ensure_structured_chats_of(&env, window, cx);
        }
    }

    fn ensure_structured_chats_of(
        &mut self,
        env: &EnvironmentName,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.envs.get_mut(env) else {
            return;
        };
        if live.pending_chats.is_empty() {
            return;
        }
        let nodes = &live.data.nodes;
        let pending: Vec<(TerminalId, LiveSession, AgentPreset)> = live
            .pending_chats
            .drain()
            .filter_map(|(id, session)| {
                let node = nodes.iter().find(|node| node.id == id)?;
                let harness = node
                    .harness
                    .filter(|preset| preset.has_structured_chat())
                    .unwrap_or(DEFAULT_CHAT_HARNESS);
                Some((id, session, harness))
            })
            .collect();
        for (id, session, harness) in pending {
            let (input, events) = Self::chat_input(id, window, cx);
            let history = session.history().cloned();
            let mut structured = StructuredChat::new(session, harness);
            let live = self.envs.get_mut(env);
            let (waiting, mode, choices) = live.map_or_else(Default::default, |live| {
                (
                    live.waiting_prompts.remove(&id).unwrap_or_default(),
                    live.waiting_modes.remove(&id),
                    live.waiting_choices.remove(&id).unwrap_or_default(),
                )
            });
            let choice = self.restored_choice(
                env,
                id,
                harness,
                ChatChoice {
                    model: choices.model,
                    effort: choices.effort,
                },
            );
            self.keep_choice(env, id, choice.clone());
            structured.permission_mode = mode;
            structured.chosen_model = choice.model;
            structured.mcp_servers = choices.mcp_servers;
            structured.effort = choice.effort;
            structured.transcript = self.acp_transcript(env, id, harness);
            let mut chat =
                SurfaceChat::new(input, events, Backend::Structured(Box::new(structured)));
            let prompted = !waiting.is_empty();
            for prompt in waiting {
                chat.outbox.push(prompt);
            }
            if let Some(live) = self.envs.get_mut(env) {
                live.chats.insert(id, chat);
            }
            self.watch_subagents(env, id, cx);
            if let AgentPreset::Acp(agent) = harness {
                self.seed_acp_chats();
                self.list_acp_models(agent, cx);
            }
            if let Some(resume) = history {
                if self.reads_history_now(env, harness) {
                    self.load_history(env.clone(), id, harness, resume, cx);
                } else {
                    self.note_history_waits_for_host(env, id);
                }
            }
            if prompted {
                self.flush_structured(env, id, cx);
            }
        }
    }

    fn reads_history_now(&self, env: &EnvironmentName, harness: AgentPreset) -> bool {
        !reads_history_on_host(harness) || self.host_agents_known(env)
    }

    fn note_history_waits_for_host(&mut self, env: &EnvironmentName, id: TerminalId) {
        let Some(destination) = self.envs.get(env).map(|live| live.data.host.to_string()) else {
            return;
        };
        if let Some(chat) = structured_chat(self, env, id) {
            chat.problem = Some(format!("{CONNECTING_TEXT} {destination}"));
        }
    }

    pub(in crate::environment_shell) fn load_missing_histories(
        &mut self,
        env: &EnvironmentName,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.envs.get_mut(env) else {
            return;
        };
        let missing: Vec<(TerminalId, AgentPreset, ResumeId)> = live
            .chats
            .iter_mut()
            .filter(|(_, chat)| chat.messages.is_empty())
            .filter_map(|(id, chat)| {
                let Backend::Structured(structured) = &chat.backend else {
                    return None;
                };
                let resume = structured.session.history()?.clone();
                let harness = structured.harness;
                chat.problem = None;
                Some((*id, harness, resume))
            })
            .collect();
        for (id, harness, resume) in missing {
            self.load_history(env.clone(), id, harness, resume, cx);
        }
    }

    fn load_history(
        &mut self,
        env: EnvironmentName,
        id: TerminalId,
        harness: AgentPreset,
        resume: ResumeId,
        cx: &mut Context<Self>,
    ) {
        match harness {
            AgentPreset::ClaudeCode => self.load_structured_history(env, id, resume, cx),
            AgentPreset::Opencode => self.load_opencode_history(env, id, resume, cx),
            AgentPreset::Codex => self.load_codex_history(env, id, resume, cx),
            AgentPreset::Gateway => self.load_gateway_history(env, id, resume, cx),
            AgentPreset::Acp(_) => Self::load_acp_history(env, id, cx),
            AgentPreset::Shell => {}
        }
    }

    pub(super) fn note_chat_resume(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        resume: ResumeId,
    ) {
        let Some(node) = self
            .envs
            .get_mut(env)
            .and_then(|live| live.data.nodes.iter_mut().find(|node| node.id == id))
        else {
            return;
        };
        if node.resume.as_ref() == Some(&resume) {
            return;
        }
        node.resume = Some(resume);
        self.store();
    }

    fn load_opencode_history(
        &mut self,
        env: EnvironmentName,
        id: TerminalId,
        resume: ResumeId,
        cx: &mut Context<Self>,
    ) {
        if let Some(OpencodeLink::Failed(error)) = self.opencode_link_of(&env) {
            let error = error.clone();
            if let Some(chat) = structured_chat(self, &env, id) {
                chat.problem = Some(error);
            }
            return;
        }
        let Some(server) = self.opencode_server_of(&env).cloned() else {
            self.start_host_opencode_for(&env, id, cx);
            self.opencode.waiting_histories.push((env, id, resume));
            return;
        };
        let read = cx.background_spawn(async move {
            read_opencode_transcript(&server, &resume).map_err(|error| error.to_string())
        });
        Self::show_history_when_read(env, id, read, cx);
    }

    fn load_codex_history(
        &mut self,
        env: EnvironmentName,
        id: TerminalId,
        resume: ResumeId,
        cx: &mut Context<Self>,
    ) {
        let root = self
            .envs
            .get(&env)
            .and_then(|live| live.data.group_of(id))
            .map(|group| group.root.clone());
        let (runner, cwd) =
            match self.harness_runner(&self.env_link(&env), AgentPreset::Codex, root.as_deref()) {
                Ok(found) => found,
                Err(error) => {
                    if let Some(chat) = structured_chat(self, &env, id) {
                        chat.problem = Some(error.to_string());
                    }
                    return;
                }
            };
        let read = cx.background_spawn(async move {
            read_codex_transcript(&runner, &cwd, &resume).map_err(|error| error.to_string())
        });
        Self::show_history_when_read(env, id, read, cx);
    }

    fn load_gateway_history(
        &mut self,
        env: EnvironmentName,
        id: TerminalId,
        resume: ResumeId,
        cx: &mut Context<Self>,
    ) {
        let Some(sessions) = gateway_sessions_dir() else {
            return;
        };
        let cwd = self.chat_folder(&env, id);
        let read = cx.background_spawn(async move {
            read_engine_transcript(&sessions, &cwd, &resume).map_err(|error| error.to_string())
        });
        Self::show_history_when_read(env, id, read, cx);
    }

    fn load_acp_history(env: EnvironmentName, id: TerminalId, cx: &mut Context<Self>) {
        let Some(file) = acp_transcript_file(&env, id) else {
            return;
        };
        let read = cx.background_spawn(async move {
            read_acp_transcript(&file).map_err(|error| error.to_string())
        });
        Self::show_history_when_read(env, id, read, cx);
    }

    fn show_history_when_read(
        env: EnvironmentName,
        id: TerminalId,
        read: Task<Result<Vec<ChatEntry>, String>>,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            let read = read.await;
            this.update(cx, |shell, cx| {
                let Some(chat) =
                    structured_chat(shell, &env, id).filter(|chat| chat.messages.is_empty())
                else {
                    return;
                };
                match read {
                    Ok(entries) => {
                        prepend_history(chat, &entries, None);
                        if let Backend::Structured(structured) = &mut chat.backend {
                            structured.permission_mode = last_permission_mode(&entries);
                        }
                        chat.thread.scroll_to_end();
                    }
                    Err(error) => chat.problem = Some(error),
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(in crate::environment_shell) fn load_waiting_histories(&mut self, cx: &mut Context<Self>) {
        for (env, id, resume) in std::mem::take(&mut self.opencode.waiting_histories) {
            self.load_opencode_history(env, id, resume, cx);
        }
    }

    fn load_structured_history(
        &mut self,
        env: EnvironmentName,
        id: TerminalId,
        resume: ResumeId,
        cx: &mut Context<Self>,
    ) {
        let root = self
            .envs
            .get(&env)
            .and_then(|live| live.data.group_of(id))
            .map(|group| group.root.clone());
        let job = TailJob {
            link: self.env_link(&env),
            resume,
            root,
            home: None,
            file: None,
            from: 0,
            first_look: true,
        };
        let read = cx.background_spawn(async move { job.run() });
        cx.spawn(async move |this, cx| {
            let read = read.await;
            this.update(cx, |shell, cx| {
                if let Some(chat) = shell
                    .envs
                    .get_mut(&env)
                    .and_then(|live| live.chats.get_mut(&id))
                    .filter(|chat| chat.messages.is_empty())
                {
                    chat.problem.clone_from(&read.problem);
                    fold_history(chat, &read, !read.skips_history());
                    if let Backend::Structured(structured) = &mut chat.backend {
                        structured.earlier = Earlier::of(&read);
                        structured.permission_mode = last_permission_mode(&read.chunk.entries);
                    }
                    chat.thread.scroll_to_end();
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(in crate::environment_shell::surface) fn show_earlier_structured(
        &mut self,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        let Some(earlier) = structured_mut(self, &env, id).and_then(|chat| chat.earlier.take())
        else {
            return;
        };
        let link = self.env_link(&env);
        let read = cx.background_spawn(async move {
            let entries = read_claude_transcript_before(&link, &earlier.file, earlier.end);
            (entries, started_at(&link, &earlier.file))
        });
        cx.spawn(async move |this, cx| {
            let (entries, started) = read.await;
            this.update(cx, |shell, cx| {
                let Some(chat) = structured_chat(shell, &env, id) else {
                    return;
                };
                match entries {
                    Ok(entries) => prepend_history(chat, &entries, started),
                    Err(error) => chat.problem = Some(error.to_string()),
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::reads_history_on_host;
    use krad_domain::AgentPreset;

    #[test]
    fn only_harnesses_that_keep_history_on_the_host_wait_for_it() {
        assert!(reads_history_on_host(AgentPreset::ClaudeCode));
        assert!(reads_history_on_host(AgentPreset::Codex));
        assert!(reads_history_on_host(AgentPreset::Opencode));
        assert!(!reads_history_on_host(AgentPreset::Gateway));
    }
}
