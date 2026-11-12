use gpui_kit::{AppContext as _, Context, Task};
use krad_chat::{
    read_claude_transcript_before, read_codex_transcript, read_engine_transcript,
    read_opencode_transcript, Runner,
};
use krad_domain::{AgentPreset, ChatEntry, EnvironmentName, ResumeId, TerminalId};
use krad_store::{acp_transcript_file, gateway_sessions_dir, read_acp_transcript};
use std::path::PathBuf;
use std::time::SystemTime;

use super::super::super::super::chat::{fold_entry, last_permission_mode};
use super::super::super::super::cloud::NodeSite;
use super::super::super::super::opencode::OpencodeLink;
use super::super::super::super::EnvironmentShell;
use super::super::super::transcript::{fold_history, started_at, TailJob, TailRead};
use super::super::super::{Backend, Origin, SurfaceChat};
use super::super::{structured_chat, structured_mut};

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

impl EnvironmentShell {
    pub(super) fn load_history(
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
        let server = match self.node_site(&env, id) {
            NodeSite::Env => self.opencode_server_of(&env).cloned(),
            NodeSite::Cloud(site) => {
                let server = self.cloud_opencode_server(site.placement.runner).cloned();
                if server.is_none() {
                    self.ensure_cloud_opencode(&site, cx);
                    self.opencode.waiting_histories.push((env, id, resume));
                    return;
                }
                server
            }
            NodeSite::Waiting(runner) => return self.note_cloud_wait(&env, id, runner, cx),
        };
        let Some(server) = server else {
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
        let found = match self.node_site(&env, id) {
            NodeSite::Env => {
                self.harness_runner(&self.env_link(&env), AgentPreset::Codex, root.as_deref())
            }
            NodeSite::Cloud(site) => self.launch_program(AgentPreset::Codex).map(|program| {
                let runner = Runner::Ssh {
                    link: site.link,
                    program,
                };
                (runner, site.placement.worktree)
            }),
            NodeSite::Waiting(runner) => return self.note_cloud_wait(&env, id, runner, cx),
        };
        let (runner, cwd) = match found {
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
        let (link, root) = match self.node_site(&env, id) {
            NodeSite::Env => (
                self.env_link(&env),
                self.envs
                    .get(&env)
                    .and_then(|live| live.data.group_of(id))
                    .map(|group| group.root.clone()),
            ),
            NodeSite::Cloud(site) => (site.link, Some(site.placement.worktree)),
            NodeSite::Waiting(runner) => return self.note_cloud_wait(&env, id, runner, cx),
        };
        let job = TailJob {
            link,
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
        let link = match self.node_site(&env, id) {
            NodeSite::Cloud(site) => site.link,
            NodeSite::Env | NodeSite::Waiting(_) => self.env_link(&env),
        };
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
