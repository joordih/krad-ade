use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, FontWeight, InteractiveElement, IntoElement, MouseButton,
    ParentElement, SharedString, StatefulInteractiveElement, Styled, Window,
};
use krad_domain::{
    relative_age, AttentionKind, AttentionTarget, InboxFilter, SessionId, Visibility,
};
use std::time::Instant;

use super::super::dialog_parts::{segment, SegmentSize};
use super::super::EnvironmentShell;
use crate::actions::ToggleAttentionInbox;
use crate::components::floating::floating_surface;
use crate::components::{icon, IconName};
use crate::theme::{
    palette, Palette, ICON_SM, LINE_SM, LINE_XS, NAV_ROW_H, RADIUS_XS, SPACE_1, SPACE_1_5, SPACE_2,
    SPACE_3, TEXT_SM, TEXT_XS,
};

const INBOX_ID: &str = "attention-inbox";
const INBOX_LIST_ID: &str = "attention-inbox-list";
const FILTER_PREFIX: &str = "attention-filter";
const WAITING_ROW_PREFIX: &str = "attention-waiting";
const HISTORY_ROW_PREFIX: &str = "attention-history";
const CLEAR_ID: &str = "attention-clear-history";
const INBOX_W: f32 = 380.0;
const INBOX_MAX_H: f32 = 520.0;
const INBOX_TOP: f32 = 44.0;
const INBOX_RIGHT: f32 = 12.0;
const EDGE_MARGIN: f32 = 8.0;
const INBOX_TITLE: &str = "Attention";
const WAITING_HEADING: &str = "Waiting for you";
const HISTORY_HEADING: &str = "History";
const CLEAR_LABEL: &str = "Clear history";
const NOTHING_WAITING: &str = "Nothing is waiting for you";
const NO_HISTORY: &str = "Nothing has happened yet";
const AGE_SEPARATOR: &str = " · ";
const UNTITLED_AGENT: &str = "Agent";

struct InboxRow {
    target: AttentionTarget,
    kind: AttentionKind,
    title: String,
    age: String,
}

const fn kind_icon(kind: AttentionKind) -> IconName {
    match kind {
        AttentionKind::Permission => IconName::Lock,
        AttentionKind::Ask => IconName::Comment,
        AttentionKind::Error => IconName::CancelCircle,
        AttentionKind::Finished => IconName::CheckCircle,
    }
}

const fn kind_tone(kind: AttentionKind, pal: &Palette) -> u32 {
    match kind {
        AttentionKind::Permission | AttentionKind::Ask => pal.attention,
        AttentionKind::Error => pal.error_fg,
        AttentionKind::Finished => pal.text_muted,
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn on_toggle_attention_inbox(
        &mut self,
        _: &ToggleAttentionInbox,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_attention_inbox(cx);
    }

    pub(in crate::environment_shell) fn toggle_attention_inbox(&mut self, cx: &mut Context<Self>) {
        self.attention.inbox = self.attention.inbox.toggle();
        cx.notify();
    }

    pub(in crate::environment_shell) fn attention_waiting(&self) -> bool {
        !self.attention.queue.is_empty()
    }

    pub(in crate::environment_shell) const fn attention_inbox_open(&self) -> bool {
        self.attention.inbox.is_visible()
    }

    fn close_attention_inbox(&mut self, cx: &mut Context<Self>) {
        self.attention.inbox = Visibility::Hidden;
        cx.notify();
    }

    fn agent_title(&self, target: &AttentionTarget) -> Option<String> {
        self.envs
            .get(&target.environment)?
            .data
            .nodes
            .iter()
            .find(|node| node.id == target.terminal)
            .map(|node| node.title.clone())
    }

    fn waiting_rows(&self, now: Instant) -> Vec<InboxRow> {
        let filter = self.attention.filter;
        self.attention
            .queue
            .ordered()
            .into_iter()
            .filter(|entry| filter.admits(entry.kind))
            .map(|entry| InboxRow {
                target: entry.target.clone(),
                kind: entry.kind,
                title: self.agent_title(&entry.target).unwrap_or_default(),
                age: relative_age(now.duration_since(entry.since)),
            })
            .collect()
    }

    fn history_rows(&self, now: Instant) -> Vec<InboxRow> {
        self.attention
            .log
            .filtered(self.attention.filter)
            .map(|record| InboxRow {
                target: record.target.clone(),
                kind: record.kind,
                title: record.title.clone(),
                age: relative_age(now.duration_since(record.at)),
            })
            .collect()
    }

    pub(in crate::environment_shell) fn attention_inbox_overlay(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.attention.inbox.is_visible() {
            return None;
        }
        let pal = palette(cx);
        let (_, root_h) = self.root_size(window);
        let height = INBOX_MAX_H.min((root_h - INBOX_TOP - EDGE_MARGIN).max(0.0));
        let now = Instant::now();
        let waiting = self.waiting_rows(now);
        let history = self.history_rows(now);
        let dismiss = |this: &mut Self,
                       _: &gpui_kit::MouseDownEvent,
                       _: &mut Window,
                       cx: &mut Context<Self>| this.close_attention_inbox(cx);
        let waiting_rows: Vec<AnyElement> = if waiting.is_empty() {
            vec![empty_line(NOTHING_WAITING, pal)]
        } else {
            waiting
                .into_iter()
                .enumerate()
                .map(|(index, row)| Self::inbox_row((WAITING_ROW_PREFIX, index), row, pal, cx))
                .collect()
        };
        let history_rows: Vec<AnyElement> = if history.is_empty() {
            vec![empty_line(NO_HISTORY, pal)]
        } else {
            history
                .into_iter()
                .enumerate()
                .map(|(index, row)| Self::inbox_row((HISTORY_ROW_PREFIX, index), row, pal, cx))
                .collect()
        };
        Some(
            div()
                .absolute()
                .inset_0()
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .occlude()
                        .on_mouse_down(MouseButton::Left, cx.listener(dismiss))
                        .on_mouse_down(MouseButton::Right, cx.listener(dismiss)),
                )
                .child(
                    floating_surface(pal, cx)
                        .id(INBOX_ID)
                        .absolute()
                        .top(px(INBOX_TOP))
                        .right(px(INBOX_RIGHT))
                        .w(px(INBOX_W))
                        .max_h(px(height))
                        .occlude()
                        .child(self.inbox_header(pal, cx))
                        .child(self.inbox_filters(pal, cx))
                        .child(
                            div()
                                .id(INBOX_LIST_ID)
                                .flex()
                                .flex_col()
                                .flex_1()
                                .min_h_0()
                                .overflow_y_scroll()
                                .px(px(SPACE_1_5))
                                .pb(px(SPACE_2))
                                .child(heading(WAITING_HEADING, pal))
                                .children(waiting_rows)
                                .child(heading(HISTORY_HEADING, pal))
                                .children(history_rows),
                        ),
                )
                .into_any_element(),
        )
    }

    fn inbox_header(&self, pal: &Palette, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px(px(SPACE_3))
            .pt(px(SPACE_3))
            .pb(px(SPACE_2))
            .child(
                div()
                    .text_size(px(TEXT_SM))
                    .line_height(px(LINE_SM))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(pal.text_strong))
                    .child(INBOX_TITLE),
            )
            .when(!self.attention.log.is_empty(), |header| {
                header.child(
                    div()
                        .id(CLEAR_ID)
                        .cursor_pointer()
                        .rounded(px(RADIUS_XS))
                        .px(px(SPACE_1_5))
                        .text_size(px(TEXT_XS))
                        .line_height(px(LINE_XS))
                        .text_color(rgb(pal.text_muted))
                        .hover(|style| style.bg(pal.bg_hover))
                        .child(CLEAR_LABEL)
                        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                            this.attention.log.clear();
                            cx.notify();
                        })),
                )
            })
            .into_any_element()
    }

    fn inbox_filters(&self, pal: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let current = self.attention.filter;
        let chips = InboxFilter::ALL.iter().enumerate().map(|(index, &filter)| {
            segment(
                (FILTER_PREFIX, index),
                filter.display_name(),
                None,
                current == filter,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.attention.filter = filter;
                cx.notify();
            }))
        });
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(px(SPACE_1))
            .px(px(SPACE_3))
            .pb(px(SPACE_2))
            .children(chips)
            .into_any_element()
    }

    fn inbox_row(
        id: (&'static str, usize),
        row: InboxRow,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let title = if row.title.is_empty() {
            UNTITLED_AGENT.to_owned()
        } else {
            row.title
        };
        let detail = [row.target.environment.as_str(), row.age.as_str()].join(AGE_SEPARATOR);
        let target = row.target;
        div()
            .id(id)
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .h(px(NAV_ROW_H))
            .px(px(SPACE_1_5))
            .rounded(px(RADIUS_XS))
            .cursor_pointer()
            .hover(|style| style.bg(pal.bg_hover))
            .child(icon(kind_icon(row.kind), ICON_SM, kind_tone(row.kind, pal)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(TEXT_SM))
                    .line_height(px(LINE_SM))
                    .text_color(rgb(pal.text_default))
                    .child(SharedString::from(title)),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .text_color(rgb(pal.text_muted))
                    .child(SharedString::from(detail)),
            )
            .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                this.close_attention_inbox(cx);
                this.open_attention(&target, window, cx);
            }))
            .into_any_element()
    }

    pub(in crate::environment_shell) fn toggle_session_attention(
        &mut self,
        session: SessionId,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        if let Some(live) = self.envs.get_mut(&env) {
            live.data.toggle_session_attention(session);
        }
        cx.notify();
        self.store();
    }
}

fn heading(text: &'static str, pal: &Palette) -> AnyElement {
    div()
        .pt(px(SPACE_2))
        .pb(px(SPACE_1))
        .px(px(SPACE_1_5))
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(pal.text_subtle))
        .child(text)
        .into_any_element()
}

fn empty_line(text: &'static str, pal: &Palette) -> AnyElement {
    div()
        .px(px(SPACE_1_5))
        .py(px(SPACE_1))
        .text_size(px(TEXT_SM))
        .line_height(px(LINE_SM))
        .text_color(rgb(pal.text_muted))
        .child(text)
        .into_any_element()
}
