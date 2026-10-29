use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::ActiveTheme;
use gpui_kit::{
    div, px, rgb, AnyElement, AppContext as _, Context, Entity, FontWeight, InteractiveElement,
    IntoElement, ParentElement, StatefulInteractiveElement, Styled, Subscription, Window,
};
use krad_domain::review::{Discussion, DiscussionId, Note};

use super::avatar;
use super::detail::RequestDetail;
use super::summary::short_time;
use crate::components::{icon, Button, ButtonVariant, IconName};
use crate::environment_shell::EnvironmentShell;
use crate::theme::{
    card_shadow, palette, Palette, INPUT_H, LINE_2XS, LINE_XS, RADIUS_LG, RADIUS_SM, SPACE_1_5,
    SPACE_2, SPACE_3, TEXT_2XS, TEXT_SM, TEXT_XS,
};

const THREAD_GAP: f32 = 10.0;
const NOTE_LINE: f32 = 19.0;
const NOTE_HEAD_GAP: f32 = 2.0;
const PLACE_ICON: f32 = 12.0;
const RESOLVED_ICON: f32 = 13.0;
const CHEVRON: f32 = 12.0;
const RESOLVED_ROW_H: f32 = 36.0;
const REPLY_PLACEHOLDER: &str = "Reply…";
const RESOLVE_LABEL: &str = "Resolve";
const RESOLVE_THREAD_LABEL: &str = "Resolve thread";
const THREAD_PREFIX: &str = "review-thread";
const REPLY_PREFIX: &str = "review-reply";
const RESOLVE_PREFIX: &str = "review-resolve";
const RESOLVED_PREFIX: &str = "review-resolved";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ThreadPlace {
    Overview,
    Diff,
}

pub(in crate::environment_shell) struct ReplyDraft {
    pub(super) discussion: DiscussionId,
    pub(super) field: Entity<InputState>,
    _send_on_enter: Subscription,
}

impl EnvironmentShell {
    pub(super) fn thread(
        index: usize,
        discussion: &Discussion,
        detail: &RequestDetail,
        place: ThreadPlace,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if discussion.is_resolved() && !detail.expanded.contains(&discussion.id) {
            return Self::resolved_row(index, discussion, cx);
        }
        let pal = palette(cx);
        let mono = cx.theme().mono_font_family.clone();
        let notes = comments(discussion).map(|note| note_entry(note, pal));
        let where_on_code = (place == ThreadPlace::Overview)
            .then(|| note_place(discussion))
            .flatten()
            .map(|place| {
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(SPACE_1_5))
                    .font_family(mono)
                    .text_size(px(TEXT_2XS))
                    .line_height(px(LINE_2XS))
                    .text_color(rgb(pal.text_muted))
                    .child(icon(IconName::File, PLACE_ICON, pal.text_muted))
                    .child(place)
            });
        let reply = Self::reply_field(index, discussion, detail, cx);
        let resolve = (discussion.is_resolvable() && !discussion.is_resolved()).then(|| {
            let id = discussion.id.clone();
            let label = match place {
                ThreadPlace::Overview => RESOLVE_LABEL,
                ThreadPlace::Diff => RESOLVE_THREAD_LABEL,
            };
            Button::new((RESOLVE_PREFIX, index as u64))
                .label(label)
                .small()
                .variant(ButtonVariant::Secondary)
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.resolve_thread(id.clone(), cx);
                }))
        });
        let actions = match place {
            ThreadPlace::Overview => div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(SPACE_2))
                .child(div().flex_1().child(reply))
                .children(resolve)
                .into_any_element(),
            ThreadPlace::Diff => div()
                .flex()
                .flex_col()
                .gap(px(THREAD_GAP))
                .child(reply)
                .children(resolve.map(|resolve| div().flex().flex_row().child(resolve)))
                .into_any_element(),
        };
        div()
            .id((THREAD_PREFIX, index as u64))
            .flex()
            .flex_col()
            .gap(px(THREAD_GAP))
            .p(px(SPACE_3))
            .rounded(px(RADIUS_LG))
            .bg(rgb(pal.bg_surface))
            .shadow(card_shadow(pal))
            .border_1()
            .border_color(pal.border)
            .children(where_on_code)
            .children(notes)
            .child(actions)
            .into_any_element()
    }

    fn resolved_row(index: usize, discussion: &Discussion, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let first = comments(discussion).next();
        let summary = first.map_or_else(String::new, |note| {
            format!(
                "Resolved · \"{}\" · @{}",
                note.body.lines().next().unwrap_or_default(),
                note.author.username
            )
        });
        let id = discussion.id.clone();
        div()
            .id((RESOLVED_PREFIX, index as u64))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .h(px(RESOLVED_ROW_H))
            .px(px(SPACE_3))
            .rounded(px(RADIUS_LG))
            .bg(rgb(pal.bg_panel))
            .cursor_pointer()
            .text_size(px(TEXT_XS))
            .text_color(rgb(pal.text_muted))
            .child(icon(IconName::CheckCircle, RESOLVED_ICON, pal.text_muted))
            .child(div().flex_1().min_w_0().truncate().child(summary))
            .child(icon(IconName::ChevronDown, CHEVRON, pal.text_muted))
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                if let Some(detail) = this.review.detail.as_mut() {
                    detail.expanded.insert(id.clone());
                }
                cx.notify();
            }))
            .into_any_element()
    }

    fn reply_field(
        index: usize,
        discussion: &Discussion,
        detail: &RequestDetail,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        if let Some(reply) = detail
            .reply
            .as_ref()
            .filter(|reply| reply.discussion == discussion.id)
        {
            return Input::new(&reply.field).into_any_element();
        }
        let id = discussion.id.clone();
        div()
            .id((REPLY_PREFIX, index as u64))
            .flex()
            .items_center()
            .h(px(INPUT_H))
            .px(px(THREAD_GAP))
            .rounded(px(RADIUS_SM))
            .bg(rgb(pal.bg_panel))
            .cursor_text()
            .text_size(px(TEXT_SM))
            .text_color(rgb(pal.text_muted))
            .child(REPLY_PLACEHOLDER)
            .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                this.start_reply(id.clone(), window, cx);
            }))
            .into_any_element()
    }

    fn start_reply(
        &mut self,
        discussion: DiscussionId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let field = cx.new(|cx| InputState::new(window, cx).placeholder(REPLY_PLACEHOLDER));
        let send_on_enter = cx.subscribe_in(
            &field,
            window,
            |this: &mut Self, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.send_reply(window, cx);
                }
            },
        );
        field.update(cx, |field, cx| field.focus(window, cx));
        if let Some(detail) = self.review.detail.as_mut() {
            detail.reply = Some(ReplyDraft {
                discussion,
                field,
                _send_on_enter: send_on_enter,
            });
        }
        cx.notify();
    }
}

pub(super) fn comments(discussion: &Discussion) -> impl Iterator<Item = &Note> {
    discussion.notes.iter().filter(|note| !note.system)
}

fn note_place(discussion: &Discussion) -> Option<String> {
    let position = discussion.notes.first()?.position.as_ref()?;
    let path = position.new_path.as_ref().or(position.old_path.as_ref())?;
    let line = position.new_line.or(position.old_line)?;
    Some(format!("{path}:{line}"))
}

fn note_entry(note: &Note, pal: &Palette) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .gap(px(THREAD_GAP))
        .child(avatar(&note.author, pal))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap(px(NOTE_HEAD_GAP))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .text_size(px(TEXT_XS))
                        .line_height(px(LINE_XS))
                        .text_color(rgb(pal.text_muted))
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(pal.text_strong))
                                .child(format!("@{}", note.author.username)),
                        )
                        .child(format!(" · {}", short_time(&note.created_at))),
                )
                .child(
                    div()
                        .text_size(px(TEXT_SM))
                        .line_height(px(NOTE_LINE))
                        .text_color(rgb(pal.text_strong))
                        .child(note.body.clone()),
                ),
        )
        .into_any_element()
}
