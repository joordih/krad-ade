use gpui_kit::component::ActiveTheme;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, ClipboardItem, Context, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, ParentElement, SharedString, StatefulInteractiveElement, Styled, Window,
};
use krad_domain::{relative_age, Visibility};
use std::collections::VecDeque;
use std::time::Instant;

mod tabs;

pub(super) use tabs::{TerminalsTab, UtilityTab};

use super::drag::DragState;
use super::session_pane::DraggedTab;
use super::EnvironmentShell;
use crate::actions::ToggleActivity;
use crate::components::{card, icon, tooltip_view, Button, IconName};
use crate::theme::{
    palette, CARD_HEADER_H, ICON_XS, LINE_SM, LINE_XS, RADIUS_XS, SPACE_1, SPACE_1_5, SPACE_2,
    TEXT_SM, TEXT_XS, WINDOW_INSET,
};

const ACTIVITY_PANEL_H: f32 = 168.0;
const ACTIVITY_MIN_H: f32 = 96.0;
const ACTIVITY_MAX_H: f32 = 640.0;
const HEIGHT_HANDLE_H: f32 = 6.0;
const COPY_BUTTON: f32 = 20.0;
const LIST_ID: &str = "activity-list";
const PANEL_ID: &str = "activity-panel";
const HANDLE_ID: &str = "activity-handle";
const TOOLTIP_OPEN: &str = "Utilities";
const HEADER_PAD_X: f32 = SPACE_1_5;
const LOG_PAD_X: f32 = SPACE_1_5 + SPACE_2;
const TOOLTIP_KEEP_OPEN: &str = "Keep the utilities open";
const LEAVE_SLACK: f32 = 24.0;
const HANDLE_W: f32 = 44.0;
const HANDLE_H: f32 = 12.0;
const ROW_PREFIX: &str = "activity-row";
const COPY_PREFIX: &str = "activity-copy";
const BTN_COPY_ALL: &str = "activity-copy-all";
const TOOLTIP_COPY: &str = "Copy";
const TOOLTIP_COPY_ALL: &str = "Copy all";
const TOOLTIP_CLOSE: &str = "Close";
const COPIED_TEXT: &str = "Copied to the clipboard";
const LINE_BREAK: &str = "\n";
const ACTIVITY_AGE_W: f32 = 56.0;
const ACTIVITY_CAP: usize = 200;
const ACTIVITY_EMPTY: &str = "No events yet";
const BTN_ACTIVITY_CLOSE: &str = "activity-close";
const EVENT_ROW_H: f32 = 24.0;

pub(super) struct ActivityEntry {
    text: String,
    at: Instant,
}

pub(super) struct ActivityLog {
    entries: VecDeque<ActivityEntry>,
    height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ActivityDrag {
    mouse_start: f32,
    height_start: f32,
}

impl ActivityDrag {
    pub(super) fn height_at(self, y: f32) -> f32 {
        self.height_start + (self.mouse_start - y)
    }
}

impl Default for ActivityLog {
    fn default() -> Self {
        Self {
            entries: VecDeque::new(),
            height: ACTIVITY_PANEL_H,
        }
    }
}

impl ActivityLog {
    pub(super) fn record(&mut self, text: String) {
        self.entries.push_front(ActivityEntry {
            text,
            at: Instant::now(),
        });
        self.entries.truncate(ACTIVITY_CAP);
    }

    pub(super) const fn set_height(&mut self, height: f32) {
        self.height = height.clamp(ACTIVITY_MIN_H, ACTIVITY_MAX_H);
    }

    fn as_text(&self) -> String {
        self.entries
            .iter()
            .rev()
            .map(|entry| entry.text.as_str())
            .collect::<Vec<_>>()
            .join(LINE_BREAK)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ActivityOpened {
    Deliberately,
    ByHover,
}

impl EnvironmentShell {
    fn full_activity_room(&self) -> f32 {
        self.activity_log.height + WINDOW_INSET
    }

    pub(super) fn activity_room(&self) -> f32 {
        self.full_activity_room() * self.activity_reveal.fraction()
    }

    pub(super) fn activity_shift(&self) -> f32 {
        if !self.activity_reveal.is_moving() {
            return 0.0;
        }
        let sized_for = if self.activity.is_visible() {
            0.0
        } else {
            self.full_activity_room()
        };
        sized_for - self.activity_room()
    }

    pub(super) fn follow_pointer_for_activity(
        &mut self,
        y: f32,
        window_h: f32,
        cx: &mut Context<Self>,
    ) {
        if !self.activity.is_visible()
            || self.activity_opened != ActivityOpened::ByHover
            || matches!(self.drag, DragState::ResizeActivity(_))
        {
            return;
        }
        let top = window_h - WINDOW_INSET - self.activity_log.height - LEAVE_SLACK;
        if y < top {
            self.activity = Visibility::Hidden;
            self.activity_opened = ActivityOpened::Deliberately;
            cx.notify();
        }
    }

    pub(super) fn follow_activity_drag(&mut self, y: f32, cx: &mut Context<Self>) {
        let DragState::ResizeActivity(drag) = self.drag else {
            return;
        };
        self.activity_log.set_height(drag.height_at(y));
        cx.notify();
    }

    pub(super) fn release_activity_drag(&mut self, cx: &mut Context<Self>) {
        if matches!(self.drag, DragState::ResizeActivity(_)) {
            self.end_drag(cx);
        }
    }

    pub(super) fn toggle_activity(&mut self, cx: &mut Context<Self>) {
        self.activity = self.activity.toggle();
        self.activity_opened = ActivityOpened::Deliberately;
        cx.notify();
    }

    pub(super) fn activity_handle(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let (glyph, tooltip) = match (self.activity.is_visible(), self.activity_opened) {
            (false, _) => (IconName::ChevronUp, TOOLTIP_OPEN),
            (true, ActivityOpened::ByHover) => (IconName::ChevronUp, TOOLTIP_KEEP_OPEN),
            (true, ActivityOpened::Deliberately) => (IconName::ChevronDown, TOOLTIP_CLOSE),
        };
        div()
            .absolute()
            .bottom_0()
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(
                div()
                    .id(HANDLE_ID)
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(HANDLE_W))
                    .h(px(HANDLE_H))
                    .rounded_t(px(RADIUS_XS))
                    .cursor_pointer()
                    .occlude()
                    .hover(|style| style.bg(pal.bg_hover))
                    .tooltip(move |_, cx| tooltip_view(tooltip.into(), cx))
                    .on_hover(cx.listener(|this: &mut Self, hovered: &bool, _, cx| {
                        if *hovered && !this.activity.is_visible() {
                            this.activity = Visibility::Visible;
                            this.activity_opened = ActivityOpened::ByHover;
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        if this.activity.is_visible()
                            && this.activity_opened == ActivityOpened::ByHover
                        {
                            this.activity_opened = ActivityOpened::Deliberately;
                            cx.notify();
                        } else {
                            this.toggle_activity(cx);
                        }
                    }))
                    .child(icon(glyph, ICON_XS, pal.text_muted)),
            )
            .into_any_element()
    }

    pub(super) fn on_toggle_activity(
        &mut self,
        _: &ToggleActivity,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_activity(cx);
    }

    pub(super) fn sync_activity(&mut self, now: Instant) -> bool {
        if self.activity_reveal.is_open() != self.activity.is_visible() {
            self.activity_reveal.toggle(now);
        }
        self.activity_reveal.advance(now)
    }

    pub(super) fn activity_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let shown = self.activity_reveal.fraction();
        if shown <= 0.0 {
            return None;
        }
        let pal = palette(cx);
        let now = Instant::now();
        let rows: Vec<AnyElement> = self
            .activity_log
            .entries
            .iter()
            .enumerate()
            .map(|(index, entry)| Self::activity_row(index, entry, now, cx))
            .collect();
        let empty = rows.is_empty();
        let height = self.activity_log.height;
        let docked = self.activity_terminal();
        let panel = card(pal, cx)
            .id(PANEL_ID)
            .relative()
            .flex_none()
            .h(px(height))
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_none()
                    .items_center()
                    .gap(px(SPACE_2))
                    .h(px(CARD_HEADER_H))
                    .px(px(HEADER_PAD_X))
                    .child(self.activity_tabs(cx))
                    .child(div().flex_1())
                    .children(self.add_terminal_button(cx))
                    .when(!empty && docked.is_none(), |header| {
                        header.child(
                            Button::new(BTN_COPY_ALL)
                                .icon(IconName::Copy)
                                .small()
                                .tooltip(TOOLTIP_COPY_ALL)
                                .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                    let all = this.activity_log.as_text();
                                    this.copy_activity(all, cx);
                                })),
                        )
                    })
                    .child(
                        Button::new(BTN_ACTIVITY_CLOSE)
                            .icon(IconName::Close)
                            .small()
                            .tooltip(TOOLTIP_CLOSE)
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.toggle_activity(cx);
                            })),
                    ),
            )
            .when_some(docked, |panel, id| {
                panel.child(self.utility_terminal_view(id, cx))
            })
            .when(docked.is_none(), |panel| {
                panel.child(
                    div()
                        .id(LIST_ID)
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_h_0()
                        .px(px(LOG_PAD_X))
                        .pb(px(SPACE_2))
                        .overflow_y_scroll()
                        .children(rows)
                        .when(empty, |list| {
                            list.text_color(rgb(pal.text_muted)).child(ACTIVITY_EMPTY)
                        }),
                )
            })
            .child(Self::height_handle(height, cx))
            .on_drop(cx.listener(|this: &mut Self, dragged: &DraggedTab, _, cx| {
                cx.stop_propagation();
                this.dock_in_activity(dragged.id, cx);
            }));
        let panel = tabs::drop_ready(panel, pal.accent);
        Some(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom(px(-(1.0 - shown) * height))
                .h(px(height))
                .occlude()
                .child(panel)
                .into_any_element(),
        )
    }

    fn activity_row(
        index: usize,
        entry: &ActivityEntry,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let mono = cx.theme().mono_font_family.clone();
        let text = entry.text.clone();
        let row_group = SharedString::from(format!("{ROW_PREFIX}-{index}"));
        div()
            .id((ROW_PREFIX, index))
            .group(row_group.clone())
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SPACE_2))
            .min_h(px(EVENT_ROW_H))
            .px(px(SPACE_1))
            .rounded(px(RADIUS_XS))
            .hover(|style| style.bg(pal.bg_hover))
            .child(
                div()
                    .flex_none()
                    .w(px(ACTIVITY_AGE_W))
                    .font_family(mono.clone())
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .text_color(rgb(pal.text_muted))
                    .child(relative_age(now.saturating_duration_since(entry.at))),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_color(rgb(pal.text_strong))
                    .child(entry.text.clone()),
            )
            .child(
                div()
                    .id((COPY_PREFIX, index))
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(px(COPY_BUTTON))
                    .rounded(px(RADIUS_XS))
                    .invisible()
                    .group_hover(row_group, Styled::visible)
                    .hover(|style| style.bg(pal.bg_selected))
                    .tooltip(|_, cx| tooltip_view(TOOLTIP_COPY.into(), cx))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.copy_activity(text.clone(), cx);
                    }))
                    .child(icon(IconName::Copy, ICON_XS, pal.text_muted)),
            )
            .into_any_element()
    }

    fn height_handle(height: f32, cx: &mut Context<Self>) -> AnyElement {
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(HEIGHT_HANDLE_H))
            .cursor_row_resize()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this: &mut Self, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.drag = DragState::ResizeActivity(ActivityDrag {
                        mouse_start: event.position.y.as_f32(),
                        height_start: height,
                    });
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn copy_activity(&mut self, text: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.push_toast(COPIED_TEXT.to_owned());
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_panel_height_stays_within_its_limits() {
        let mut log = ActivityLog::default();
        log.set_height(ACTIVITY_MAX_H * 2.0);
        assert!((log.height - ACTIVITY_MAX_H).abs() < f32::EPSILON);
        log.set_height(0.0);
        assert!((log.height - ACTIVITY_MIN_H).abs() < f32::EPSILON);
    }

    #[test]
    fn copy_all_lists_the_entries_oldest_first() {
        let mut log = ActivityLog::default();
        log.record("first".to_owned());
        log.record("second".to_owned());
        assert_eq!(log.as_text(), "first\nsecond");
    }
}
