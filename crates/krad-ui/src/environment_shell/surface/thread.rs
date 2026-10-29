use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{div, list, px, AnyElement, Context, IntoElement, ParentElement, Styled, Window};
use krad_chat::Speaker;
use krad_domain::TerminalId;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::Rc;
use std::time::Instant;

use super::super::chat::{question_bubble, AnswerPart, ChatMessage};
use super::super::EnvironmentShell;
use super::view::{
    column, error_line, muted_line, origin_line, status_line, AgentNow, LOADING_TEXT,
};
use super::{Origin, SurfaceChat, THINKING_LABEL, WORKING_LABEL};
use crate::components::{Button, ButtonVariant};
use crate::theme::palette;

const THREAD_PAD_TOP: f32 = 20.0;
const THREAD_PAD_BOTTOM: f32 = 28.0;
const MESSAGE_GAP: f32 = 22.0;
const EARLIER_PREFIX: &str = "surface-earlier";
const EARLIER_LABEL: &str = "Show earlier messages";

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum ThreadItem {
    Earlier,
    Origin(Origin),
    Message(usize),
    Loading,
    Pending(usize),
    Status,
    Problem,
    Notice,
}

fn thread_items(chat: &SurfaceChat, now: AgentNow) -> Vec<ThreadItem> {
    let mut items: Vec<ThreadItem> = Vec::new();
    if chat.skips_history() {
        items.push(ThreadItem::Earlier);
    }
    items.extend(chat.origin.map(ThreadItem::Origin));
    items.extend((0..chat.messages.len()).map(ThreadItem::Message));
    if chat.messages.is_empty() && chat.problem.is_none() && chat.outbox.messages.is_empty() {
        items.push(ThreadItem::Loading);
    }
    items.extend((0..chat.outbox.messages.len()).map(ThreadItem::Pending));
    if now == AgentNow::Working {
        items.push(ThreadItem::Status);
    }
    if chat.problem.is_some() {
        items.push(ThreadItem::Problem);
    }
    let terminal_backed = chat.structured().is_none();
    let waits_unseen = now == AgentNow::Waiting && chat.asks().is_empty();
    if terminal_backed && (waits_unseen || now == AgentNow::Exited) {
        items.push(ThreadItem::Notice);
    }
    items
}

fn status_label(last: Option<&ChatMessage>) -> &'static str {
    let wrote = last
        .filter(|message| message.speaker == Speaker::Assistant)
        .and_then(|message| message.parts.last());
    match wrote {
        Some(AnswerPart::Tool(_) | AnswerPart::Text(_) | AnswerPart::Ask(_)) => WORKING_LABEL,
        Some(AnswerPart::Thinking(_)) | None => THINKING_LABEL,
    }
}

fn volatile_start(items: &[ThreadItem]) -> usize {
    items
        .iter()
        .rposition(|item| matches!(item, ThreadItem::Message(_)))
        .unwrap_or(0)
}

fn tail_fingerprint(chat: &SurfaceChat, items: &[ThreadItem]) -> u64 {
    let mut hasher = DefaultHasher::new();
    items.hash(&mut hasher);
    if let Some(message) = chat.messages.last() {
        message.text.len().hash(&mut hasher);
        message.running.hash(&mut hasher);
        message.error.is_some().hash(&mut hasher);
        message.parts.len().hash(&mut hasher);
        if let Some(AnswerPart::Text(text) | AnswerPart::Thinking(text)) = message.parts.last() {
            text.len().hash(&mut hasher);
        }
    }
    hasher.finish()
}

fn sync_thread(chat: &SurfaceChat, items: &[ThreadItem]) {
    let thread = &chat.thread;
    let known = thread.item_count();
    let count = items.len();
    if count > known {
        thread.splice(known..known, count - known);
    } else if count < known {
        thread.splice(count..known, 0);
    }
    let fingerprint = tail_fingerprint(chat, items);
    if chat.measured_tail.replace(fingerprint) != fingerprint {
        thread.remeasure_items(volatile_start(items)..count);
    }
}

impl EnvironmentShell {
    pub(super) fn surface_thread(
        id: TerminalId,
        chat: &SurfaceChat,
        now: AgentNow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let items: Rc<[ThreadItem]> = thread_items(chat, now).into();
        sync_thread(chat, &items);
        let last = items.len().saturating_sub(1);
        list(
            chat.thread.clone(),
            cx.processor(move |this: &mut Self, index: usize, window, cx| {
                let row = items
                    .get(index)
                    .and_then(|item| this.thread_row(id, *item, window, cx))
                    .unwrap_or_else(|| div().into_any_element());
                div()
                    .w_full()
                    .flex()
                    .justify_center()
                    .child(
                        column()
                            .when(index == 0, |row| row.pt(px(THREAD_PAD_TOP)))
                            .pb(px(if index == last {
                                THREAD_PAD_BOTTOM
                            } else {
                                MESSAGE_GAP
                            }))
                            .child(row),
                    )
                    .into_any_element()
            }),
        )
        .size_full()
        .into_any_element()
    }

    fn thread_row(
        &self,
        id: TerminalId,
        item: ThreadItem,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let chat = self.envs.get(&self.active)?.chats.get(&id)?;
        let pal = palette(cx);
        Some(match item {
            ThreadItem::Earlier => Button::new((EARLIER_PREFIX, id.as_u64()))
                .label(EARLIER_LABEL)
                .small()
                .variant(ButtonVariant::Secondary)
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.show_earlier(id, cx);
                }))
                .into_any_element(),
            ThreadItem::Origin(origin) => origin_line(origin, pal),
            ThreadItem::Message(index) => {
                let message = chat.messages.get(index)?;
                match message.speaker {
                    Speaker::User => question_bubble(&message.text, false, pal),
                    Speaker::Assistant => Self::surface_answer(id, index, message, chat, cx),
                }
            }
            ThreadItem::Loading => muted_line(LOADING_TEXT, pal),
            ThreadItem::Pending(index) => {
                Self::pending_bubble(id, index, chat.outbox.messages.get(index)?, cx)
            }
            ThreadItem::Status => {
                let activity = self
                    .envs
                    .get(&self.active)
                    .and_then(|live| live.sessions.get(&id))
                    .and_then(|session| session.activity.as_deref());
                let started = chat.working_since.get().unwrap_or_else(Instant::now);
                chat.working_since.set(Some(started));
                let mono = cx.theme().mono_font_family.clone();
                status_line(
                    id.as_u64(),
                    status_label(chat.messages.last()),
                    started,
                    activity,
                    mono,
                    pal,
                )
            }
            ThreadItem::Problem => error_line(chat.problem.as_deref()?, pal),
            ThreadItem::Notice => Self::agent_notice(id, self.agent_now(id), cx)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rows_that_can_change_start_at_the_last_message() {
        let items = [
            ThreadItem::Message(0),
            ThreadItem::Message(1),
            ThreadItem::Pending(0),
            ThreadItem::Status,
        ];
        assert_eq!(volatile_start(&items), 1);
        assert_eq!(volatile_start(&[ThreadItem::Loading]), 0);
    }
}
