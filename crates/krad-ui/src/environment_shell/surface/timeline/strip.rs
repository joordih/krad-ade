use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use krad_domain::{ChatBookmarks, TerminalId};

use super::super::super::EnvironmentShell;
use super::super::SurfaceChat;
use super::{current_question, Tick, TickKind};
use crate::components::{icon, IconName};
use crate::theme::{
    palette, popover_fill, popover_shadow, Palette, ICON_SM, LINE_SM, NAV_ROW_H, RADIUS_MD,
    RADIUS_XS, SPACE_1, SPACE_2, TEXT_SM,
};

const MIN_QUESTIONS: usize = 2;
const STRIP_W: f32 = 20.0;
const STRIP_INSET: f32 = SPACE_1;
const STRIP_PAD_Y: f32 = SPACE_2;
const TICK_HIT_H: f32 = 7.0;
const TICK_H: f32 = 2.0;
const TICK_W: f32 = 12.0;
const CURRENT_TICK_W: f32 = 18.0;
const CARD_W: f32 = 300.0;
const CARD_MAX_H: f32 = 360.0;
const CARD_GAP: f32 = SPACE_1;
const TIMELINE_PREFIX: &str = "surface-timeline";
const TIMELINE_LIST_PREFIX: &str = "surface-timeline-list";
const TIMELINE_ROW_PREFIX: &str = "surface-timeline-row";

fn tick_color(kind: TickKind, current: bool, pal: &Palette) -> u32 {
    match kind {
        TickKind::Bookmark => pal.attention,
        TickKind::Question if current => pal.text_strong,
        TickKind::Question => pal.text_disabled,
    }
}

fn tick_mark(tick: &Tick, current: bool, pal: &Palette) -> gpui_kit::Div {
    let width = if current { CURRENT_TICK_W } else { TICK_W };
    div()
        .h(px(TICK_HIT_H))
        .flex()
        .items_center()
        .justify_end()
        .child(
            div()
                .h(px(TICK_H))
                .w(px(width))
                .rounded_full()
                .bg(rgb(tick_color(tick.kind, current, pal))),
        )
}

impl EnvironmentShell {
    pub(in crate::environment_shell::surface) fn timeline_strip(
        &self,
        id: TerminalId,
        chat: &SurfaceChat,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let empty = ChatBookmarks::default();
        let bookmarks = self.chat_bookmarks(id).unwrap_or(&empty);
        let ticks = chat.timeline_ticks(bookmarks);
        let questions = ticks
            .iter()
            .filter(|tick| tick.kind == TickKind::Question)
            .count();
        if questions < MIN_QUESTIONS {
            return None;
        }
        let current = current_question(&ticks, chat.thread.logical_scroll_top().item_ix);
        let open = chat.timeline.borrow().open;
        let pal = palette(cx);
        let marks = ticks
            .iter()
            .enumerate()
            .map(|(index, tick)| tick_mark(tick, current == Some(index), pal));
        let strip = div()
            .flex()
            .flex_col()
            .w(px(STRIP_W))
            .py(px(SPACE_1))
            .cursor_pointer()
            .children(marks);
        let card = open.then(|| Self::timeline_card(id, &ticks, current, pal, cx));
        Some(
            div()
                .absolute()
                .top(px(STRIP_PAD_Y))
                .bottom(px(STRIP_PAD_Y))
                .right(px(STRIP_INSET))
                .flex()
                .items_center()
                .child(
                    div()
                        .id((TIMELINE_PREFIX, id.as_u64()))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(CARD_GAP))
                        .on_hover(cx.listener(move |this: &mut Self, hovered: &bool, _, cx| {
                            this.open_timeline(id, *hovered);
                            cx.notify();
                        }))
                        .children(card)
                        .child(strip),
                )
                .into_any_element(),
        )
    }

    fn timeline_card(
        id: TerminalId,
        ticks: &[Tick],
        current: Option<usize>,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = ticks.iter().enumerate().map(|(index, tick)| {
            let target = tick.target;
            let selected = current == Some(index);
            let color = if selected {
                pal.text_strong
            } else {
                pal.text_default
            };
            div()
                .id((TIMELINE_ROW_PREFIX, index))
                .flex()
                .flex_row()
                .flex_none()
                .items_center()
                .gap(px(SPACE_2))
                .h(px(NAV_ROW_H))
                .px(px(SPACE_2))
                .rounded(px(RADIUS_XS))
                .cursor_pointer()
                .text_size(px(TEXT_SM))
                .line_height(px(LINE_SM))
                .text_color(rgb(color))
                .when(selected, |row| row.bg(pal.bg_selected))
                .hover(|row| row.bg(pal.bg_hover))
                .when(tick.kind == TickKind::Bookmark, |row| {
                    row.child(icon(IconName::Bookmark, ICON_SM, pal.attention))
                })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(tick.label.clone()),
                )
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.jump_on_timeline(id, target);
                    this.open_timeline(id, false);
                    cx.notify();
                }))
        });
        div()
            .id((TIMELINE_LIST_PREFIX, id.as_u64()))
            .w(px(CARD_W))
            .max_h(px(CARD_MAX_H))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .p(px(SPACE_1))
            .rounded(px(RADIUS_MD))
            .bg(popover_fill(pal, cx))
            .border_1()
            .border_color(pal.border)
            .shadow(popover_shadow(pal, cx))
            .occlude()
            .children(rows)
            .into_any_element()
    }

    fn open_timeline(&mut self, id: TerminalId, open: bool) {
        if let Some(chat) = self
            .envs
            .get_mut(&self.active)
            .and_then(|live| live.chats.get_mut(&id))
        {
            chat.timeline.borrow_mut().open = open;
        }
    }

    fn jump_on_timeline(&mut self, id: TerminalId, row: usize) {
        if let Some(chat) = self
            .envs
            .get_mut(&self.active)
            .and_then(|live| live.chats.get_mut(&id))
        {
            chat.jump_to_row(row);
        }
    }
}
