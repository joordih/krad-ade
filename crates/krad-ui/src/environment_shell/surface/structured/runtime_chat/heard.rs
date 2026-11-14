use gpui_kit::Context;
use krad_domain::{
    AgentState, AttentionKind, ChatLogChange, ChatMessage, Mention, RuntimeId, TerminalId,
};
use krad_protocol::{AgentRef, ChatAsk, ChatState, ChatSummary, EventKind};
use std::time::Instant;

use super::super::super::super::ask::Waiting;
use super::super::super::super::EnvironmentShell;
use super::super::super::{anchor, Backend, ChatPlace, ChatView};
use super::super::{structured_chat, structured_mut, StructuredChat};
use super::shown::shown_problem;

fn runtime_chat<'a>(
    shell: &'a mut EnvironmentShell,
    place: &ChatPlace,
    id: TerminalId,
) -> Option<&'a mut ChatView> {
    structured_chat(shell, place, id).filter(|chat| {
        chat.chat
            .structured()
            .is_some_and(StructuredChat::in_runtime)
    })
}

fn place_of(runtime: RuntimeId, chat: &AgentRef) -> ChatPlace {
    ChatPlace {
        runtime,
        environment: chat.environment.clone(),
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn chat_event_heard(
        &mut self,
        runtime: RuntimeId,
        kind: EventKind,
        cx: &mut Context<Self>,
    ) {
        if let EventKind::ChatImageAttached { chat, image } = kind {
            let place = place_of(runtime, &chat);
            self.attach(&place, chat.terminal, vec![Mention::Image(image)], cx);
            return self.fetch_shown_images(&place, chat.terminal, cx);
        }
        let Some(place) = kind.chat().map(|chat| place_of(runtime, chat)) else {
            return;
        };
        let place = &place;
        match kind {
            EventKind::ChatOpened { chat, state } | EventKind::ChatState { chat, state } => {
                self.runtime_chat_state(place, chat.terminal, *state, cx);
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
            } => {
                self.runtime_chat_message(place, chat.terminal, (index, *message));
                self.fetch_shown_images(place, chat.terminal, cx);
            }
            EventKind::ChatMessageAppended { chat, index, text } => {
                self.runtime_chat_appended(place, chat.terminal, index, text);
            }
            EventKind::ChatMessagesPrepended { chat, count } => {
                if let Some(structured) = structured_mut(self, place, chat.terminal) {
                    structured.shown.shifted(count);
                }
                self.page_in_earlier(place, chat.terminal, cx);
            }
            EventKind::ChatCheckpointed {
                chat,
                point,
                checkpoint,
            } => {
                if let Some(env) = place.local() {
                    let env = env.clone();
                    self.record_turn_checkpoint(&env, chat.terminal, point, checkpoint);
                }
            }
            EventKind::ChatMessagesCut { chat, len } => {
                self.runtime_chat_cut(place, chat.terminal, len, cx);
            }
            EventKind::ChatAsk(ask) => self.runtime_chat_ask(place, ask),
            EventKind::ChatAskSettled { chat, ask } => {
                if let Some(chat) = runtime_chat(self, place, chat.terminal) {
                    chat.chat.asks_mut().forget_chat_ask(ask);
                }
            }
            EventKind::ChatSpent { chat, report } => {
                if let Some(env) = place.local() {
                    let env = env.clone();
                    self.record_spend(&env, chat.terminal, report, None, cx);
                }
            }
            _ => {}
        }
    }

    pub(in crate::environment_shell) fn runtime_chats_replaced(
        &mut self,
        runtime: RuntimeId,
        summaries: Vec<ChatSummary>,
        cx: &mut Context<Self>,
    ) {
        for summary in summaries {
            let ChatSummary { state, tail, asks } = summary;
            let place = place_of(runtime, &tail.chat);
            let id = tail.chat.terminal;
            if let Some(shown) = runtime_chat(self, &place, id) {
                shown.chat.messages = tail.messages;
                shown.opened_thoughts.clear();
                shown.tool_groups.clear();
                shown.thread.remeasure();
                if let Some(structured) = shown.chat.structured_mut() {
                    structured.shown.starts_at(tail.from);
                }
            }
            for ask in asks {
                self.runtime_chat_ask(&place, ask);
            }
            self.runtime_chat_state(&place, id, state, cx);
        }
    }

    fn runtime_chat_message(
        &mut self,
        place: &ChatPlace,
        id: TerminalId,
        (index, message): (usize, ChatMessage),
    ) {
        let Some((shown, index)) = runtime_chat(self, place, id).and_then(|shown| {
            let index = shown.chat.structured()?.shown.local(index)?;
            Some((shown, index))
        }) else {
            return;
        };
        let followed = anchor::follows_tail(shown.anchor, &shown.thread);
        ChatLogChange::Changed { index, message }.apply(&mut shown.chat.messages);
        if followed {
            shown.thread.scroll_to_end();
        }
    }

    fn runtime_chat_appended(
        &mut self,
        place: &ChatPlace,
        id: TerminalId,
        index: usize,
        text: String,
    ) {
        let Some((shown, index)) = runtime_chat(self, place, id).and_then(|shown| {
            let index = shown.chat.structured()?.shown.local(index)?;
            Some((shown, index))
        }) else {
            return;
        };
        let followed = anchor::follows_tail(shown.anchor, &shown.thread);
        ChatLogChange::Appended { index, text }.apply(&mut shown.chat.messages);
        if followed {
            shown.thread.scroll_to_end();
        }
    }

    fn runtime_chat_cut(
        &mut self,
        place: &ChatPlace,
        id: TerminalId,
        len: usize,
        cx: &mut Context<Self>,
    ) {
        let Some((shown, len)) = runtime_chat(self, place, id).and_then(|shown| {
            let len = shown
                .chat
                .structured()?
                .shown
                .local(len)
                .unwrap_or_default();
            Some((shown, len))
        }) else {
            return;
        };
        shown.chat.messages.truncate(len);
        shown.opened_thoughts.retain(|(message, _)| *message < len);
        shown.tool_groups.retain(|(message, _), _| *message < len);
        shown.thread.remeasure();
        let rewound = shown
            .chat
            .structured_mut()
            .is_some_and(|structured| std::mem::take(&mut structured.rewinding));
        if rewound {
            shown.editing = None;
            shown.pins_latest = true;
            shown.start_glide(Instant::now());
            if let Some(env) = place.local() {
                let env = env.clone();
                self.files_rewind_settled(&env, id, true, cx);
            }
        }
    }

    fn runtime_chat_ask(&mut self, place: &ChatPlace, ask: ChatAsk) {
        let id = ask.chat.terminal;
        let Some(shown) = runtime_chat(self, place, id) else {
            return;
        };
        if shown.chat.asks().holds_chat_ask(ask.ask) {
            return;
        }
        shown.chat.asks_mut().push(Waiting::of_runtime(ask));
        if let Some(env) = place.local() {
            let env = env.clone();
            self.set_agent_state(&env, id, AgentState::WaitingForInput);
        }
    }

    fn runtime_chat_state(
        &mut self,
        place: &ChatPlace,
        id: TerminalId,
        state: ChatState,
        cx: &mut Context<Self>,
    ) {
        let local = place.local().cloned();
        let Some(shown) = runtime_chat(self, place, id) else {
            return;
        };
        shown.chat.problem = shown_problem(&state);
        if state.origin.is_some() {
            shown.chat.origin = state.origin;
        }
        let failed = shown
            .chat
            .messages
            .last()
            .is_some_and(|last| last.error.is_some());
        let waits = !shown.chat.asks().is_empty();
        let Backend::Structured(structured) = &mut shown.chat.backend else {
            return;
        };
        structured.shown.take_state(&state);
        let asks_sign_in = state
            .acp
            .as_ref()
            .is_some_and(|acp| structured.acp.adopt(acp));
        structured.adopt_catalog(state.commands.clone(), state.account.clone());
        structured.launch = state.launch;
        let was_working = structured.working;
        if state.working {
            structured.working = true;
        } else {
            structured.ends_turn();
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
        if let (true, Some(env)) = (asks_sign_in, &local) {
            self.set_agent_state(env, id, AgentState::WaitingForInput);
        }
        if was_working == state.working {
            return;
        }
        let settled = match (state.working, waits) {
            (true, _) => AgentState::Working,
            (false, true) => AgentState::WaitingForInput,
            (false, false) => AgentState::Idle,
        };
        if let Some(env) = &local {
            self.set_agent_state(env, id, settled);
            if failed && !state.working {
                self.raise_attention(env, id, AttentionKind::Error);
            }
        }
        if !state.working {
            self.flush_structured(place, id, cx);
        }
    }
}
