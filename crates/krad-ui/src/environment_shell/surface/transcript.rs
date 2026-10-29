use gpui_kit::{Context, Task, Window};
use krad_chat::{
    claude_conversation_file, read_claude_transcript, read_claude_transcript_tail, SlashCommand,
    Speaker, TranscriptChunk,
};
use krad_domain::{AgentState, ChatEntry, EnvironmentName, ResumeId, TerminalId};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use super::super::ask::AskQueue;
use super::super::chat::{fold_entry, workplace_home, ChatMessage, Composer};
use super::super::EnvironmentShell;
use super::{view, Backend, Origin, SurfaceChat, SurfaceCommand, SURFACE_HARNESS};

const TAIL_INTERVAL: Duration = Duration::from_millis(400);
const FIRST_LOOK_BYTES: u64 = 2 * 1024 * 1024;
const NO_HOME_TEXT: &str = "Cannot find the home folder Claude Code keeps conversations in";
const THINKING_TOGGLE: &[u8] = b"\x1bt";
const INTERRUPT: &[u8] = b"\x1b";

#[derive(Default)]
struct Transcript {
    resume: Option<ResumeId>,
    home: Option<PathBuf>,
    file: Option<PathBuf>,
    read: u64,
    skipped: bool,
    whole: bool,
}

pub(in crate::environment_shell) struct TranscriptView {
    transcript: Transcript,
    _tail: Task<()>,
    pub(in crate::environment_shell) asks: AskQueue,
}

pub(super) struct TailJob {
    pub(super) resume: ResumeId,
    pub(super) root: Option<PathBuf>,
    pub(super) home: Option<PathBuf>,
    pub(super) file: Option<PathBuf>,
    pub(super) from: u64,
    pub(super) first_look: bool,
}

pub(super) struct TailRead {
    pub(super) resume: ResumeId,
    pub(super) home: Option<PathBuf>,
    pub(super) file: Option<PathBuf>,
    pub(super) chunk: TranscriptChunk,
    pub(super) started: Option<SystemTime>,
    pub(super) problem: Option<String>,
    pub(super) start: u64,
}

impl TailRead {
    pub(super) const fn skips_history(&self) -> bool {
        self.start > 0
    }
}

impl TailJob {
    pub(super) fn run(self) -> TailRead {
        let mut read = TailRead {
            resume: self.resume,
            home: self.home,
            file: self.file,
            chunk: TranscriptChunk {
                entries: Vec::new(),
                next: self.from,
            },
            started: None,
            problem: None,
            start: 0,
        };
        if read.home.is_none() {
            match workplace_home(self.root.as_deref()) {
                Ok(Some(home)) => read.home = Some(home),
                Ok(None) => read.problem = Some(NO_HOME_TEXT.to_owned()),
                Err(error) => read.problem = Some(error.to_string()),
            }
        }
        if read.file.is_none() {
            read.file = read
                .home
                .as_deref()
                .and_then(|home| claude_conversation_file(home, &read.resume));
        }
        let Some(file) = read.file.clone() else {
            return read;
        };
        if self.from == 0 {
            read.started = started_at(&file);
        }
        let chunk = if self.first_look {
            read_claude_transcript_tail(&file, FIRST_LOOK_BYTES).map(|(chunk, start)| {
                read.start = start;
                chunk
            })
        } else {
            read_claude_transcript(&file, self.from)
        };
        match chunk {
            Ok(chunk) => read.chunk = chunk,
            Err(error) => read.problem = Some(error.to_string()),
        }
        read
    }
}

pub(super) fn started_at(file: &Path) -> Option<SystemTime> {
    let metadata = std::fs::metadata(file).ok()?;
    metadata.created().or_else(|_| metadata.modified()).ok()
}

fn open_answer() -> ChatMessage {
    ChatMessage {
        speaker: Speaker::Assistant,
        text: String::new(),
        tools: Vec::new(),
        parts: Vec::new(),
        preset: SURFACE_HARNESS,
        error: None,
        running: true,
    }
}

pub(super) fn fold_history(chat: &mut SurfaceChat, read: &TailRead, opened: bool) {
    if opened {
        chat.origin = Origin::of(&read.chunk.entries, read.started);
    }
    let starts_mid_answer = matches!(read.chunk.entries.first(), Some(ChatEntry::Event(_)));
    if chat.messages.is_empty() && starts_mid_answer {
        chat.messages.push(open_answer());
    }
    for entry in &read.chunk.entries {
        if matches!(entry, ChatEntry::User { .. }) {
            chat.outbox.delivered();
        }
        fold_entry(&mut chat.messages, entry);
    }
}

impl EnvironmentShell {
    fn can_switch(&self, env: &EnvironmentName, id: TerminalId) -> bool {
        self.surface_switch_of(env, id)
            .is_some_and(|switch| switch.lock.is_none())
    }

    pub(in crate::environment_shell) fn switch_to_chat(
        &mut self,
        id: TerminalId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        if !self.can_switch(&env, id) {
            return;
        }
        let (input, input_events) = Self::chat_input(id, window, cx);
        let focus_target = input.clone();
        cx.defer_in(window, move |_, window, cx| {
            focus_target.update(cx, |field, cx| field.focus(window, cx));
        });
        let view = TranscriptView {
            transcript: Transcript::default(),
            _tail: Self::tail_transcript(env.clone(), id, cx),
            asks: AskQueue::default(),
        };
        let Some(live) = self.envs.get_mut(&env) else {
            return;
        };
        live.chats.insert(
            id,
            SurfaceChat::new(input, input_events, Backend::Transcript(Box::new(view))),
        );
        self.watch_subagents(&env, id, cx);
        cx.notify();
    }

    pub(in crate::environment_shell) fn switch_to_terminal(
        &mut self,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        if let Some(live) = self.envs.get_mut(&env) {
            live.chats.remove(&id);
        }
        if self
            .chat
            .suggest
            .as_ref()
            .is_some_and(|suggest| suggest.composer == Composer::Surface(id))
        {
            self.close_suggest(cx);
        }
        cx.notify();
    }

    pub(super) fn flush_transcript(&mut self, env: &EnvironmentName, id: TerminalId) {
        let now = Instant::now();
        let Some(live) = self.envs.get_mut(env) else {
            return;
        };
        let running = live
            .sessions
            .get(&id)
            .is_some_and(|session| session.state.is_running());
        let state = live
            .data
            .nodes
            .iter()
            .find(|node| node.id == id)
            .map(|node| node.state);
        let Some(chat) = live.chats.get_mut(&id) else {
            return;
        };
        let working = matches!(
            state,
            Some(AgentState::Working | AgentState::WaitingForInput)
        );
        chat.outbox.settle(working, now);
        let free = running && matches!(state, Some(AgentState::Idle | AgentState::Done));
        if let Some(text) = chat.outbox.next_to_send(free, now) {
            self.submit_text_into(env, id, &text);
        }
    }

    pub(super) fn command_transcript(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        command: &SurfaceCommand,
    ) {
        match command {
            SurfaceCommand::Stop => self.type_into_terminal(env, id, INTERRUPT.to_vec()),
            SurfaceCommand::ToggleThinking => {
                self.type_into_terminal(env, id, THINKING_TOGGLE.to_vec());
            }
            SurfaceCommand::Model(model) => {
                let slash = SlashCommand::Model(model.clone());
                self.submit_text_into(env, id, &slash.to_string());
            }
            SurfaceCommand::Effort(effort) => {
                let slash = SlashCommand::Effort(*effort);
                self.submit_text_into(env, id, &slash.to_string());
            }
        }
    }

    fn tail_transcript(env: EnvironmentName, id: TerminalId, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| loop {
            let Ok(Some(job)) = this.update(cx, |shell, _| shell.tail_job(&env, id)) else {
                break;
            };
            let read = cx
                .background_executor()
                .spawn(async move { job.run() })
                .await;
            if this
                .update(cx, |shell, cx| {
                    shell.apply_tail(&env, id, &read, cx);
                    shell.flush_transcript(&env, id);
                })
                .is_err()
            {
                break;
            }
            cx.background_executor().timer(TAIL_INTERVAL).await;
        })
    }

    fn tail_job(&mut self, env: &EnvironmentName, id: TerminalId) -> Option<TailJob> {
        let live = self.envs.get_mut(env)?;
        let resume = live
            .data
            .nodes
            .iter()
            .find(|node| node.id == id)?
            .resume
            .clone()?;
        let root = live.data.group_of(id).map(|group| group.root.clone());
        let chat = live.chats.get_mut(&id)?;
        let Backend::Transcript(view) = &mut chat.backend else {
            return None;
        };
        let transcript = &mut view.transcript;
        if transcript.resume.as_ref() != Some(&resume) {
            transcript.resume = Some(resume.clone());
            transcript.file = None;
            transcript.read = 0;
            chat.messages.clear();
            chat.opened_thoughts.clear();
            chat.thread.remeasure();
            chat.origin = None;
        }
        Some(TailJob {
            resume,
            root,
            home: transcript.home.clone(),
            file: transcript.file.clone(),
            from: transcript.read,
            first_look: transcript.read == 0 && !transcript.whole,
        })
    }

    fn apply_tail(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        read: &TailRead,
        cx: &mut Context<Self>,
    ) {
        let Some(chat) = self
            .envs
            .get_mut(env)
            .and_then(|live| live.chats.get_mut(&id))
        else {
            return;
        };
        let Backend::Transcript(view) = &mut chat.backend else {
            return;
        };
        let transcript = &mut view.transcript;
        if transcript.resume.as_ref() != Some(&read.resume) {
            return;
        }
        transcript.home.clone_from(&read.home);
        transcript.file.clone_from(&read.file);
        let opened = transcript.read == 0;
        transcript.read = read.chunk.next;
        if opened {
            transcript.skipped = read.skips_history();
        }
        chat.problem.clone_from(&read.problem);
        if read.chunk.entries.is_empty() {
            return;
        }
        let followed = view::following(&chat.thread);
        fold_history(chat, read, opened && !read.skips_history());
        if followed {
            chat.thread.scroll_to_end();
        }
        cx.notify();
    }

    pub(super) fn show_whole_transcript(&mut self, id: TerminalId, cx: &mut Context<Self>) {
        let env = self.active.clone();
        let Some(chat) = self
            .envs
            .get_mut(&env)
            .and_then(|live| live.chats.get_mut(&id))
        else {
            return;
        };
        let Backend::Transcript(view) = &mut chat.backend else {
            return;
        };
        let transcript = &mut view.transcript;
        transcript.whole = true;
        transcript.skipped = false;
        transcript.read = 0;
        chat.messages.clear();
        chat.opened_thoughts.clear();
        chat.origin = None;
        chat.thread.remeasure();
        cx.notify();
    }
}

impl SurfaceChat {
    pub(super) fn skips_history(&self) -> bool {
        match &self.backend {
            Backend::Transcript(view) => view.transcript.skipped,
            Backend::Structured(structured) => structured.earlier.is_some(),
        }
    }
}
