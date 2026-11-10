use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    canvas, div, fill, point, px, rgb, size, AnyElement, Bounds, Context, DispatchPhase,
    InteractiveElement, IntoElement, MouseMoveEvent, ParentElement, Pixels, Point,
    StatefulInteractiveElement, Styled, Window,
};
use krad_domain::{ChatBookmarks, TerminalId};
use std::rc::Rc;

use super::super::super::EnvironmentShell;
use super::super::SurfaceChat;
use super::hover::Area;
use super::{current_question, Tick, TickKind, View};
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

fn area_watch(area: Area) -> AnyElement {
    canvas(move |bounds, _, _| area.set(Some(bounds)), |_, (), _, _| {})
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .into_any_element()
}

fn pointer_watch(id: TerminalId, area: Area, cx: &mut Context<EnvironmentShell>) -> AnyElement {
    let shell = cx.entity().downgrade();
    canvas(
        move |bounds, _, _| area.set(Some(bounds)),
        move |_, (), window, _| {
            let shell = shell.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                if phase != DispatchPhase::Bubble {
                    return;
                }
                shell
                    .update(cx, |shell, cx| shell.track_timeline(id, event.position, cx))
                    .unwrap_or_default();
            });
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
    .into_any_element()
}

fn rows(count: usize) -> f32 {
    u16::try_from(count).map_or(f32::from(u16::MAX), f32::from)
}

fn row_top(top: f32, index: usize) -> f32 {
    top + rows(index) * TICK_HIT_H
}

fn ticks_in_view(top: f32, count: usize, clip: (f32, f32)) -> impl Iterator<Item = usize> {
    (0..count)
        .skip_while(move |index| row_top(top, index + 1) <= clip.0)
        .take_while(move |index| row_top(top, *index) < clip.1)
}

#[derive(Clone, Copy)]
struct TickInk {
    bookmark: u32,
    current: u32,
    rest: u32,
}

impl TickInk {
    const fn of(pal: &Palette) -> Self {
        Self {
            bookmark: pal.attention,
            current: pal.text_strong,
            rest: pal.text_disabled,
        }
    }

    const fn color(self, kind: TickKind, current: bool) -> u32 {
        match kind {
            TickKind::Bookmark => self.bookmark,
            TickKind::Question if current => self.current,
            TickKind::Question => self.rest,
        }
    }
}

fn paint_ticks(
    bounds: Bounds<Pixels>,
    ticks: &[Tick],
    current: Option<usize>,
    ink: TickInk,
    window: &mut Window,
) {
    let mask = window.content_mask().bounds;
    let top = bounds.origin.y.as_f32();
    let clip = (mask.origin.y.as_f32(), mask.bottom().as_f32());
    let right = bounds.right();
    for index in ticks_in_view(top, ticks.len(), clip) {
        let Some(tick) = ticks.get(index) else {
            continue;
        };
        let lit = current == Some(index);
        let width = if lit { CURRENT_TICK_W } else { TICK_W };
        let y = row_top(top, index) + (TICK_HIT_H - TICK_H) / 2.0;
        let bar = Bounds::new(point(right - px(width), px(y)), size(px(width), px(TICK_H)));
        window.paint_quad(fill(bar, rgb(ink.color(tick.kind, lit))).corner_radii(px(TICK_H / 2.0)));
    }
}

fn tick_field(ticks: Rc<[Tick]>, current: Option<usize>, pal: &Palette) -> AnyElement {
    let ink = TickInk::of(pal);
    let height = rows(ticks.len()) * TICK_HIT_H;
    div()
        .w_full()
        .h(px(height))
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, (), window, _| paint_ticks(bounds, &ticks, current, ink, window),
            )
            .size_full(),
        )
        .into_any_element()
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
        let view = if chat.thread.is_scrolled_to_end() == Some(true) {
            View::AtEnd
        } else {
            View::From(chat.thread.logical_scroll_top().item_ix)
        };
        let current = current_question(&ticks, view);
        let (open, strip_area, card_area) = {
            let timeline = chat.timeline.borrow();
            let hover = &timeline.hover;
            (hover.open, hover.strip.clone(), hover.card.clone())
        };
        let pal = palette(cx);
        let marks = tick_field(ticks.clone(), current, pal);
        let strip = div()
            .relative()
            .flex()
            .flex_col()
            .w(px(STRIP_W))
            .py(px(SPACE_1))
            .cursor_pointer()
            .child(marks)
            .child(pointer_watch(id, strip_area, cx));
        let card = open.then(|| {
            div()
                .relative()
                .child(Self::timeline_card(id, &ticks, current, pal, cx))
                .child(area_watch(card_area))
        });
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
                    this.close_timeline(id);
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

    fn close_timeline(&mut self, id: TerminalId) {
        if let Some(chat) = self.active_chat(id) {
            chat.timeline.borrow_mut().hover.close();
        }
    }

    fn track_timeline(&mut self, id: TerminalId, point: Point<Pixels>, cx: &mut Context<Self>) {
        let changed = self
            .active_chat(id)
            .is_some_and(|chat| chat.timeline.borrow_mut().hover.track(point));
        if changed {
            cx.notify();
        }
    }

    fn active_chat(&self, id: TerminalId) -> Option<&SurfaceChat> {
        self.envs.get(&self.active)?.chats.get(&id)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn painted(count: usize, clip: (f32, f32)) -> Vec<usize> {
        ticks_in_view(100.0, count, clip).collect()
    }

    #[test]
    fn only_the_ticks_inside_the_clip_are_painted() {
        assert_eq!(painted(1000, (100.0, 170.0)), (0..10).collect::<Vec<_>>());
        assert_eq!(painted(1000, (103.5, 110.0)), vec![0, 1]);
        assert_eq!(
            painted(1000, (800.0, 870.0)),
            (100..110).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_clip_beyond_the_strip_paints_nothing_past_its_ends() {
        assert_eq!(painted(5, (0.0, 2000.0)), (0..5).collect::<Vec<_>>());
        assert_eq!(painted(5, (0.0, 50.0)), Vec::<usize>::new());
        assert_eq!(painted(5, (500.0, 900.0)), Vec::<usize>::new());
    }

    #[test]
    fn each_tick_keeps_its_row_of_the_strip() {
        assert!((row_top(100.0, 0) - 100.0).abs() < f32::EPSILON);
        assert!((row_top(100.0, 3) - (100.0 + 3.0 * TICK_HIT_H)).abs() < f32::EPSILON);
    }
}
