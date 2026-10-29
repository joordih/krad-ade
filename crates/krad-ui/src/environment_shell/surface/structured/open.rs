use gpui_kit::{AppContext as _, Context, Window};
use krad_chat::{new_session_id, read_claude_transcript_before, LiveSession};
use krad_domain::{ChatEntry, EnvironmentName, NodeKind, ResumeId, TerminalId, TerminalNode};
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
        let pending: Vec<(TerminalId, LiveSession)> = live
            .pending_chats
            .drain()
            .filter(|(id, _)| nodes.iter().any(|node| node.id == *id))
            .collect();
        for (id, session) in pending {
            let (input, events) = Self::chat_input(id, window, cx);
            let history = session.history().cloned();
            let chat = SurfaceChat::new(
                input,
                events,
                Backend::Structured(Box::new(StructuredChat::new(session))),
            );
            if let Some(live) = self.envs.get_mut(&env) {
                live.chats.insert(id, chat);
            }
            self.watch_subagents(&env, id, cx);
            if let Some(resume) = history {
                self.load_structured_history(env.clone(), id, resume, cx);
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
