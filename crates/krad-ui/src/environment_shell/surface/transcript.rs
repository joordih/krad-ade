use gpui_kit::{Context, Task, Window};
use krad_client::ClientError;
use krad_domain::SlashCommand;
use krad_domain::{
    AgentPreset, AgentState, ChatEntry, ChatEvent, EnvironmentName, ResumeId, Speaker, TerminalId,
    ToolUseId, TranscriptChunk, TranscriptMark,
};
use krad_protocol::{Answer, ChatCommand, Command, TranscriptAsk, TranscriptRead};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::super::ask::{AskQueue, Settled};
use super::super::chat::{fold_entry, ChatMessage};
use super::super::live_terminal::LiveTerminal;
use super::super::EnvironmentShell;
use super::{anchor, Backend, ChatView, SurfaceCommand};

pub(super) const TRANSCRIPT_HARNESS: AgentPreset = AgentPreset::ClaudeCode;
const TAIL_INTERVAL: Duration = Duration::from_millis(400);
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
    calls: HashMap<ToolUseId, String>,
}

impl Transcript {
    fn settle_marked(&mut self, chunk: &TranscriptChunk, asks: &mut AskQueue, live: bool) -> bool {
        let named = chunk.entries.iter().filter_map(|entry| match entry {
            ChatEntry::Event(ChatEvent::Tool(call)) => {
                call.id.clone().map(|id| (id, call.name.clone()))
            }
            _ => None,
        });
        self.calls.extend(named);
        chunk.marks.iter().fold(false, |changed, mark| {
            let settled = match mark {
                TranscriptMark::Interrupted => live && asks.settle(Settled::All),
                TranscriptMark::ToolFinished(call) => {
                    let tool = self.calls.remove(call);
                    live && asks.settle(Settled::Call {
                        call,
                        tool: tool.as_deref(),
                    })
                }
            };
            changed || settled
        })
    }
}

pub(in crate::environment_shell) struct TranscriptView {
    transcript: Transcript,
    _tail: Task<()>,
    pub(in crate::environment_shell) asks: AskQueue,
}

fn open_answer() -> ChatMessage {
    ChatMessage {
        speaker: Speaker::Assistant,
        text: String::new(),
        parts: Vec::new(),
        error: None,
        running: true,
        mentions: Vec::new(),
        checklist: None,
        rewind: None,
        tool_outputs: HashMap::new(),
    }
}

pub(super) fn fold_history(chat: &mut ChatView, read: &TranscriptRead, opened: bool) {
    if opened {
        chat.chat.origin = krad_domain::ChatOrigin::of(&read.chunk.entries, read.started);
    }
    let starts_mid_answer = matches!(read.chunk.entries.first(), Some(ChatEntry::Event(_)));
    if chat.chat.messages.is_empty() && starts_mid_answer {
        chat.chat.messages.push(open_answer());
    }
    for entry in &read.chunk.entries {
        if matches!(entry, ChatEntry::User { .. }) {
            chat.chat.outbox.delivered();
        }
        fold_entry(&mut chat.chat.messages, entry);
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
            ChatView::new(input, input_events, Backend::Transcript(Box::new(view))),
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
        self.leave_runtime(&env, id);
        if let Some(live) = self.envs.get_mut(&env) {
            live.chats.remove(&id);
        }
        if self
            .chat
            .suggest
            .as_ref()
            .is_some_and(|suggest| suggest.composer == id)
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
        let running = live.sessions.get(&id).is_some_and(LiveTerminal::is_running);
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
        chat.chat.outbox.settle(working, now);
        let free = running && matches!(state, Some(AgentState::Idle | AgentState::Done));
        if let Some(prompt) = chat.chat.outbox.next_to_send(free, now) {
            self.submit_text_into(env, id, &prompt.text);
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
            SurfaceCommand::Model(_, model) => {
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
            let Ok(Some(asked)) = this.update(cx, |shell, cx| {
                let ask = shell.tail_ask(&env, id)?;
                let command = Command::Chat(ChatCommand::Transcript {
                    environment: env.clone(),
                    ask,
                });
                Some(shell.runtime.ask(command, cx))
            }) else {
                break;
            };
            let read = match asked.await {
                Ok(Answer::Transcript(read)) => read,
                Err(ClientError::Stopped) => break,
                Ok(_) | Err(_) => {
                    cx.background_executor().timer(TAIL_INTERVAL).await;
                    continue;
                }
            };
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

    fn tail_ask(&mut self, env: &EnvironmentName, id: TerminalId) -> Option<TranscriptAsk> {
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
        let Backend::Transcript(view) = &mut chat.chat.backend else {
            return None;
        };
        let transcript = &mut view.transcript;
        if transcript.resume.as_ref() != Some(&resume) {
            transcript.resume = Some(resume.clone());
            transcript.file = None;
            transcript.read = 0;
            transcript.calls.clear();
            chat.chat.messages.clear();
            chat.opened_thoughts.clear();
            chat.thread.remeasure();
            chat.chat.origin = None;
        }
        Some(TranscriptAsk {
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
        read: &TranscriptRead,
        cx: &mut Context<Self>,
    ) {
        let Some(chat) = self
            .envs
            .get_mut(env)
            .and_then(|live| live.chats.get_mut(&id))
        else {
            return;
        };
        let Backend::Transcript(view) = &mut chat.chat.backend else {
            return;
        };
        let transcript = &mut view.transcript;
        if transcript.resume.as_ref() != Some(&read.resume) {
            return;
        }
        let live = transcript.read > 0;
        if transcript.settle_marked(&read.chunk, &mut view.asks, live) {
            cx.notify();
        }
        transcript.home.clone_from(&read.home);
        transcript.file.clone_from(&read.file);
        let opened = transcript.read == 0;
        transcript.read = read.chunk.next;
        if opened {
            transcript.skipped = read.skips_history();
        }
        chat.chat.problem.clone_from(&read.problem);
        if read.chunk.entries.is_empty() {
            return;
        }
        let followed = anchor::follows_tail(chat.anchor, &chat.thread);
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
        let Backend::Transcript(view) = &mut chat.chat.backend else {
            return;
        };
        let transcript = &mut view.transcript;
        transcript.whole = true;
        transcript.skipped = false;
        transcript.read = 0;
        chat.chat.messages.clear();
        chat.opened_thoughts.clear();
        chat.chat.origin = None;
        chat.thread.remeasure();
        cx.notify();
    }
}

impl ChatView {
    pub(super) fn skips_history(&self) -> bool {
        match &self.chat.backend {
            Backend::Transcript(view) => view.transcript.skipped,
            Backend::Structured(structured) => structured.runtime_skips_history(),
        }
    }
}
