use gpui_kit::{AppContext as _, Context, Task, Window};
use krad_chat::{
    new_session_id, read_claude_transcript_before, read_codex_transcript, read_engine_transcript,
    read_opencode_transcript, LiveSession,
};
use krad_domain::{
    AgentPreset, ChatEntry, EnvironmentName, NodeKind, ResumeId, TerminalId, TerminalNode,
};
use krad_store::gateway_sessions_dir;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

use super::super::super::chat::{fold_entry, last_permission_mode};
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

impl EnvironmentShell {
    pub(in crate::environment_shell) fn ensure_structured_chats(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        let Some(live) = self.envs.get_mut(&env) else {
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
                    .filter(|preset| {
                        matches!(
                            preset,
                            AgentPreset::Opencode | AgentPreset::Codex | AgentPreset::Gateway
                        )
                    })
                    .unwrap_or(AgentPreset::ClaudeCode);
                Some((id, session, harness))
            })
            .collect();
        for (id, session, harness) in pending {
            let (input, events) = Self::chat_input(id, window, cx);
            let history = session.history().cloned();
            let chat = SurfaceChat::new(
                input,
                events,
                Backend::Structured(Box::new(StructuredChat::new(session, harness))),
            );
            if let Some(live) = self.envs.get_mut(&env) {
                live.chats.insert(id, chat);
            }
            self.watch_subagents(&env, id, cx);
            match (history, harness) {
                (Some(resume), AgentPreset::Opencode) => {
                    self.load_opencode_history(env.clone(), id, resume, cx);
                }
                (Some(resume), AgentPreset::Codex) => {
                    self.load_codex_history(env.clone(), id, resume, cx);
                }
                (Some(resume), AgentPreset::Gateway) => {
                    self.load_gateway_history(env.clone(), id, resume, cx);
                }
                (Some(resume), _) => self.load_structured_history(env.clone(), id, resume, cx),
                (None, _) => {}
            }
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
        let Some(server) = self.opencode_server().cloned() else {
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
        let (runner, cwd) = match self.harness_runner(AgentPreset::Codex, root.as_deref()) {
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
        let read = cx.background_spawn(async move {
            let entries = read_claude_transcript_before(&earlier.file, earlier.end);
            (entries, started_at(&earlier.file))
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
