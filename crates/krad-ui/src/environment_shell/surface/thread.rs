use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, list, px, AnyElement, Context, ElementId, InteractiveElement, IntoElement, ParentElement,
    Styled, Window,
};
use krad_chat::Speaker;
use krad_domain::TerminalId;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::Rc;
use std::time::Instant;

use super::super::chat::{question_bubble, AnswerPart, ChatMessage};
use super::super::EnvironmentShell;
use super::edit_message::is_edited;
use super::mentions::sent_mentions;
use super::message_actions::{copy_button, message_actions, message_group};
use super::origin::origin_line;
use super::view::{column, error_line, muted_line, status_line, AgentNow, LOADING_TEXT};
use super::{Backend, Origin, SurfaceChat, THINKING_LABEL, WORKING_LABEL};
use crate::components::{Button, ButtonVariant};
use crate::theme::{palette, Palette, SPACE_0_5};

const THREAD_PAD_TOP: f32 = 20.0;
const THREAD_PAD_BOTTOM: f32 = 28.0;
const MESSAGE_GAP: f32 = 22.0;
const RUNWAY_SETTLE: f32 = 0.5;
const EARLIER_PREFIX: &str = "surface-earlier";
const BUBBLE_PREFIX: &str = "surface-question";
const EARLIER_LABEL: &str = "Show earlier messages";
const QUESTION_COPY_PREFIX: &str = "surface-question-copy";
const QUESTION_COPY_TOOLTIP: &str = "Copy message";

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum ThreadItem {
    Earlier,
    Origin(Origin),
    Message(usize),
    Loading,
    Pending(usize),
    Status,
    Problem,
    Runway,
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
    if chat.pins_latest && latest_question(chat, &items).is_some() {
        items.push(ThreadItem::Runway);
    }
    items
}

fn latest_question(chat: &SurfaceChat, items: &[ThreadItem]) -> Option<usize> {
    items.iter().rposition(|item| match item {
        ThreadItem::Message(index) => chat
            .messages
            .get(*index)
            .is_some_and(|message| message.speaker == Speaker::User),
        ThreadItem::Pending(_) => true,
        _ => false,
    })
}

fn first_question(chat: &SurfaceChat, items: &[ThreadItem]) -> Option<usize> {
    items.iter().position(|item| match item {
        ThreadItem::Message(index) => chat
            .messages
            .get(*index)
            .is_some_and(|message| message.speaker == Speaker::User),
        ThreadItem::Pending(_) => true,
        _ => false,
    })
}

fn question_text(text: &str) -> Option<&str> {
    Some(text.trim()).filter(|text| !text.is_empty())
}

fn question_row(
    id: TerminalId,
    index: usize,
    message: &ChatMessage,
    chat: &SurfaceChat,
    edit: Option<AnyElement>,
    pal: &Palette,
) -> AnyElement {
    let bubble = format!("{BUBBLE_PREFIX}-{}-{index}", id.as_u64());
    let group = message_group(id, index);
    let copy = question_text(&message.text)
        .filter(|_| matches!(chat.backend, Backend::Structured(_)))
        .map(|text| {
            let key = format!("{QUESTION_COPY_PREFIX}-{}-{index}", id.as_u64());
            let copy = copy_button(
                ElementId::Name(key.into()),
                text.to_owned(),
                QUESTION_COPY_TOOLTIP,
                pal,
            );
            message_actions(group.clone(), edit.into_iter().chain([copy]))
        });
    div()
        .group(group)
        .flex()
        .flex_col()
        .w_full()
        .items_end()
        .gap(px(SPACE_0_5))
        .when(!message.text.is_empty(), |column| {
            column.child(question_bubble(
                ElementId::Name(bubble.into()),
                &message.text,
                false,
                pal,
            ))
        })
        .children(sent_mentions(&message.mentions, pal))
        .children(copy)
        .into_any_element()
}

fn runway_height(viewport: f32, below: Option<f32>, lead: f32, previous: f32) -> f32 {
    below.map_or(previous, |below| {
        (viewport - below - lead - THREAD_PAD_BOTTOM).max(0.0)
    })
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
        let runway = Self::size_runway(chat, &items);
        list(
            chat.thread.clone(),
            cx.processor(move |this: &mut Self, index: usize, window, cx| {
                let row = match items.get(index) {
                    Some(ThreadItem::Runway) => div().h(px(runway)).into_any_element(),
                    Some(item) => this
                        .thread_row(id, *item, window, cx)
                        .unwrap_or_else(|| div().into_any_element()),
                    None => div().into_any_element(),
                };
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

    fn size_runway(chat: &SurfaceChat, items: &[ThreadItem]) -> f32 {
        let Some(runway) = items.iter().position(|item| *item == ThreadItem::Runway) else {
            return 0.0;
        };
        let Some(question) = latest_question(chat, items) else {
            return 0.0;
        };
        let thread = &chat.thread;
        if chat.lead.get().is_none() {
            let first = first_question(chat, items);
            let bounds = first.and_then(|first| thread.bounds_for_item(first));
            if let (Some(first), Some(bounds), Some(top)) =
                (first, bounds, thread.bounds_for_item(0))
            {
                let padded = if first == 0 { THREAD_PAD_TOP } else { 0.0 };
                chat.lead
                    .set(Some((bounds.origin.y - top.origin.y).as_f32() + padded));
            }
        }
        let lead = chat.lead.get().unwrap_or(THREAD_PAD_TOP);
        let below: Option<f32> = (question..runway)
            .map(|index| {
                thread
                    .bounds_for_item(index)
                    .map(|bounds| bounds.size.height.as_f32())
            })
            .sum();
        let viewport = thread.viewport_bounds().size.height.as_f32();
        let height = runway_height(viewport, below, lead, chat.runway.get());
        if (height - chat.runway.get()).abs() > RUNWAY_SETTLE {
            chat.runway.set(height);
            thread.remeasure_items(runway..runway + 1);
        }
        height
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
                    Speaker::User if is_edited(chat, index) => {
                        Self::message_editor(id, chat, pal, cx)?
                    }
                    Speaker::User => {
                        let edit = chat
                            .offers_edit(index)
                            .then(|| Self::edit_button(id, index, pal, cx));
                        question_row(id, index, message, chat, edit, pal)
                    }
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
            ThreadItem::Runway => div().into_any_element(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_runway_fills_what_the_latest_question_leaves_of_the_view() {
        let lead = 60.0;
        let fitted = runway_height(600.0, Some(200.0), lead, 0.0);
        assert!((fitted - (600.0 - 200.0 - lead - THREAD_PAD_BOTTOM)).abs() < f32::EPSILON);
        assert!(runway_height(600.0, Some(900.0), lead, 120.0).abs() < f32::EPSILON);
        assert!((runway_height(600.0, None, lead, 120.0) - 120.0).abs() < f32::EPSILON);
    }

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

    #[test]
    fn copying_a_question_takes_what_was_asked_without_surrounding_space() {
        assert_eq!(
            question_text("  Fix the **build**\n\n- then test\n"),
            Some("Fix the **build**\n\n- then test")
        );
        assert_eq!(question_text(" \n "), None);
    }
}
