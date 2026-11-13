use gpui_kit::Context;
use krad_domain::{
    AgentState, AttentionKind, ChatLogChange, ChatMessage, EnvironmentName, Speaker, TerminalId,
};
use krad_protocol::{AgentRef, ChatAsk, ChatState, ChatSummary, EventKind};
use std::time::Instant;

use super::super::super::super::ask::Waiting;
use super::super::super::super::workers::LaunchProgress;
use super::super::super::super::EnvironmentShell;
use super::super::super::{anchor, Backend, SurfaceChat};
use super::super::{structured_chat, structured_mut, StructuredChat};
use super::shown::shown_problem;

fn runtime_chat<'a>(
    shell: &'a mut EnvironmentShell,
    env: &EnvironmentName,
    id: TerminalId,
) -> Option<&'a mut SurfaceChat> {
    structured_chat(shell, env, id)
        .filter(|chat| chat.structured().is_some_and(StructuredChat::in_runtime))
}

fn progress_of(message: &ChatMessage) -> LaunchProgress {
    match (&message.speaker, &message.error) {
        (Speaker::Assistant, Some(error)) => LaunchProgress::Failed(error.clone()),
        (Speaker::Assistant, None) => LaunchProgress::Started,
        (Speaker::User, _) => LaunchProgress::Unknown,
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn chat_event_heard(
        &mut self,
        kind: EventKind,
        cx: &mut Context<Self>,
    ) {
        match kind {
            EventKind::ChatOpened { chat, state } | EventKind::ChatState { chat, state } => {
                self.runtime_chat_state(&chat, *state, cx);
            }
            EventKind::ChatMessageAdded {
                chat,
                index,
                message,
            }
            | EventKind::ChatMessageChanged {
                chat,
                index,
                message,
            } => self.runtime_chat_message(&chat, index, *message, cx),
            EventKind::ChatMessageAppended { chat, index, text } => {
                self.runtime_chat_appended(&chat, index, text);
            }
            EventKind::ChatMessagesPrepended { chat, count } => {
                let (env, id) = (&chat.environment, chat.terminal);
                if let Some(structured) = structured_mut(self, env, id) {
                    structured.shown.shifted(count);
                }
                self.page_in_earlier(env, id, cx);
            }
            EventKind::ChatCheckpointed {
                chat,
                point,
                checkpoint,
            } => self.record_turn_checkpoint(&chat.environment, chat.terminal, point, checkpoint),
            EventKind::ChatMessagesCut { chat, len } => self.runtime_chat_cut(&chat, len),
            EventKind::ChatAsk(ask) => self.runtime_chat_ask(ask, cx),
            EventKind::ChatAskSettled { chat, ask } => {
                if let Some(chat) = runtime_chat(self, &chat.environment, chat.terminal) {
                    chat.asks_mut().forget_chat_ask(ask);
                }
            }
            EventKind::ChatSpent { chat, report } => {
                self.record_spend(&chat.environment, chat.terminal, report, None, cx);
            }
            _ => {}
        }
    }

    pub(in crate::environment_shell) fn runtime_chats_replaced(
        &mut self,
        summaries: Vec<ChatSummary>,
        cx: &mut Context<Self>,
    ) {
        for summary in summaries {
            let ChatSummary { state, tail, asks } = summary;
            let chat = tail.chat.clone();
            if let Some(shown) = runtime_chat(self, &chat.environment, chat.terminal) {
                shown.messages = tail.messages;
                shown.opened_thoughts.clear();
                shown.tool_groups.clear();
                shown.thread.remeasure();
                if let Some(structured) = shown.structured_mut() {
                    structured.shown.starts_at(tail.from);
                }
            }
            for ask in asks {
                self.runtime_chat_ask(ask, cx);
            }
            self.runtime_chat_state(&chat, state, cx);
        }
    }

    fn runtime_chat_message(
        &mut self,
        chat: &AgentRef,
        index: usize,
        message: ChatMessage,
        cx: &mut Context<Self>,
    ) {
        let (env, id) = (&chat.environment, chat.terminal);
        let progress = progress_of(&message);
        let Some((shown, index)) = runtime_chat(self, env, id).and_then(|shown| {
            let index = shown.structured()?.shown.local(index)?;
            Some((shown, index))
        }) else {
            return;
        };
        let followed = anchor::follows_tail(shown.anchor, &shown.thread);
        ChatLogChange::Changed { index, message }.apply(&mut shown.messages);
        if followed {
            shown.thread.scroll_to_end();
        }
        self.follow_launch_progress(env, id, progress, cx);
    }

    fn runtime_chat_appended(&mut self, chat: &AgentRef, index: usize, text: String) {
        let Some((shown, index)) =
            runtime_chat(self, &chat.environment, chat.terminal).and_then(|shown| {
                let index = shown.structured()?.shown.local(index)?;
                Some((shown, index))
            })
        else {
            return;
        };
        let followed = anchor::follows_tail(shown.anchor, &shown.thread);
        ChatLogChange::Appended { index, text }.apply(&mut shown.messages);
        if followed {
            shown.thread.scroll_to_end();
        }
    }

    fn runtime_chat_cut(&mut self, chat: &AgentRef, len: usize) {
        let Some((shown, len)) =
            runtime_chat(self, &chat.environment, chat.terminal).and_then(|shown| {
                let len = shown.structured()?.shown.local(len).unwrap_or_default();
                Some((shown, len))
            })
        else {
            return;
        };
        shown.messages.truncate(len);
        shown.opened_thoughts.retain(|(message, _)| *message < len);
        shown.tool_groups.retain(|(message, _), _| *message < len);
        shown.thread.remeasure();
        let rewound = shown
            .structured_mut()
            .and_then(|structured| structured.rewinding.take())
            .is_some();
        if rewound {
            shown.editing = None;
            shown.pins_latest = true;
            shown.start_glide(Instant::now());
        }
    }

    fn runtime_chat_ask(&mut self, ask: ChatAsk, cx: &mut Context<Self>) {
        let (env, id) = (ask.chat.environment.clone(), ask.chat.terminal);
        let Some(shown) = runtime_chat(self, &env, id) else {
            return;
        };
        if shown.asks().holds_chat_ask(ask.ask) {
            return;
        }
        shown.asks_mut().push(Waiting::of_runtime(ask));
        self.set_agent_state(&env, id, AgentState::WaitingForInput);
        self.follow_launch_progress(&env, id, LaunchProgress::Started, cx);
    }

    fn runtime_chat_state(&mut self, chat: &AgentRef, state: ChatState, cx: &mut Context<Self>) {
        let (env, id) = (&chat.environment, chat.terminal);
        if let (Some(problem), false) = (&state.problem, state.alive || state.working) {
            self.follow_launch_progress(env, id, LaunchProgress::Failed(problem.clone()), cx);
        }
        let Some(shown) = runtime_chat(self, env, id) else {
            return;
        };
        shown.problem = shown_problem(&state);
        if state.origin.is_some() {
            shown.origin = state.origin;
        }
        let failed = shown
            .messages
            .last()
            .is_some_and(|last| last.error.is_some());
        let waits = !shown.asks().is_empty();
        let Backend::Structured(structured) = &mut shown.backend else {
            return;
        };
        structured.shown.take_state(&state);
        let asks_sign_in = state
            .acp
            .as_ref()
            .is_some_and(|acp| structured.acp.adopt(acp));
        structured.adopt_catalog(state.commands.clone(), state.account.clone());
        let was_working = structured.working;
        if state.working {
            structured.working = true;
        } else {
            structured.closes_turn(true);
        }
        structured.model.clone_from(&state.model);
        structured.permission_mode = state.permission_mode;
        structured.context_tokens = state.context.tokens;
        structured.context_window = state.context.window;
        let resumed = state
            .resume
            .filter(|resume| structured.session.history() != Some(resume));
        if let Some(resume) = resumed {
            structured.session_reported(resume);
        }
        if asks_sign_in {
            self.set_agent_state(env, id, AgentState::WaitingForInput);
        }
        if was_working == state.working {
            return;
        }
        if state.working {
            self.set_agent_state(env, id, AgentState::Working);
            return;
        }
        let settled = if waits {
            AgentState::WaitingForInput
        } else {
            AgentState::Idle
        };
        self.set_agent_state(env, id, settled);
        if failed {
            self.raise_attention(env, id, AttentionKind::Error);
        }
        self.flush_structured(env, id, cx);
    }
}
