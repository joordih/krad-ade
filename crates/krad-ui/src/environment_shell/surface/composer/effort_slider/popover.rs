use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, ease_out_quint, linear_color_stop, linear_gradient, px, rgb, transparent_black, Animation,
    AnimationExt as _, AnyElement, Context, FontWeight, Hsla, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement,
    StatefulInteractiveElement, Styled, Window,
};
use krad_domain::{Effort, TerminalId};
use std::time::Duration;

use super::super::super::super::EnvironmentShell;
use super::super::super::facts::Facts;
use super::layout::{
    dot_alpha, dot_columns, fill_width, level_share, popover_left, snap_bounce, thumb_left,
    DOT_PITCH, DOT_ROWS, KEY_HELP, POPOVER_W, THUMB_W, TRACK_W,
};
use super::{current_step, level_color, EffortPlacement, DEFAULT_EFFORT_LABEL};
use crate::components::floating::floating_surface;
use crate::components::tooltip_view;
use crate::theme::{
    palette, row_shadow, tinted, Palette, LINE_BASE, LINE_XS, RADIUS_SM, SPACE_1, SPACE_1_5,
    SPACE_2, SPACE_3, TEXT_BASE, TEXT_XS,
};

const CARD_ID: &str = "effort-popover";
const TRACK_ID: &str = "effort-popover-track";
const STEP_PREFIX: &str = "effort-popover-step";
const HELP_ID: &str = "effort-popover-help";
const OPEN_PREFIX: &str = "effort-popover-open";
const LEVEL_IN_PREFIX: &str = "effort-popover-level-in";
const LEVEL_OUT_PREFIX: &str = "effort-popover-level-out";
const FILL_PREFIX: &str = "effort-popover-fill";
const THUMB_PREFIX: &str = "effort-popover-thumb";
const SHIMMER_ID: &str = "effort-popover-shimmer";
const THUMB_GROUP: &str = "effort-popover-thumb-group";
const CARD_PAD: f32 = 16.0;
const TRACK_H: f32 = 26.0;
const THUMB_INSET: f32 = 3.0;
const THUMB_GROW: f32 = 3.0;
const DOT: f32 = 3.0;
const DOT_TOP: f32 = 5.0;
const DOT_ROW_GAP: f32 = 3.0;
const HELP_SIZE: f32 = 20.0;
const ANCHOR_GAP: f32 = 10.0;
const RISE: f32 = 8.0;
const OPEN: Duration = Duration::from_millis(180);
const LEVEL_FADE: Duration = Duration::from_millis(200);
const SLIDE: Duration = Duration::from_millis(260);
const SHIMMER_PERIOD: Duration = Duration::from_millis(2600);
const SHIMMER_FPS: f32 = 24.0;
const SHIMMER_W: f32 = 48.0;
const SHIMMER_ALPHA: u32 = 0x38;
const FILL_LOW_ALPHA: u32 = 0x30;
const UNSET_FILL_OPACITY: f32 = 0.5;
const DOT_RADIUS: f32 = 1.0;
const GRADIENT_ANGLE: f32 = 90.0;
const STEP_SLOTS: u64 = 16;
const EFFORT_TITLE: &str = "Effort";
const HELP_GLYPH: &str = "?";
const TOOLTIP_HELP: &str = "What each level means for this model";
pub(super) const LOW_END: &str = "Lighter, quicker";
pub(super) const HIGH_END: &str = "Deeper, slower";
const HELP_INTRO: &str = "Levels this model offers";

fn slot(index: usize) -> u64 {
    u64::try_from(index).unwrap_or_default()
}

fn motion(reduced: bool, duration: Duration) -> Duration {
    if reduced {
        Duration::ZERO
    } else {
        duration
    }
}

fn texture(strength: f32, pal: &Palette) -> AnyElement {
    let columns = dot_columns();
    let rows = (0..DOT_ROWS).map(|row| {
        div()
            .flex()
            .flex_row()
            .children((0..columns).map(move |column| {
                let dot = div().flex_none().w(px(DOT_PITCH)).h(px(DOT));
                match dot_alpha(column, row, columns, strength) {
                    Some(alpha) => dot.child(
                        div()
                            .size(px(DOT))
                            .rounded(px(DOT_RADIUS))
                            .bg(tinted(pal.on_accent, alpha)),
                    ),
                    None => dot,
                }
            }))
    });
    div()
        .absolute()
        .left(px(DOT_PITCH / 2.0))
        .top(px(DOT_TOP))
        .w(px(TRACK_W))
        .flex()
        .flex_col()
        .gap(px(DOT_ROW_GAP))
        .children(rows)
        .into_any_element()
}

fn shimmer(pal: &Palette) -> AnyElement {
    let glint: Hsla = tinted(pal.on_accent, SHIMMER_ALPHA).into();
    div()
        .absolute()
        .top_0()
        .bottom_0()
        .w(px(SHIMMER_W))
        .bg(linear_gradient(
            GRADIENT_ANGLE,
            linear_color_stop(transparent_black(), 0.0),
            linear_color_stop(glint, 1.0),
        ))
        .with_animation(
            SHIMMER_ID,
            Animation::new(SHIMMER_PERIOD)
                .repeat_synced()
                .with_max_fps(SHIMMER_FPS),
            |band, phase| band.left(px((TRACK_W + SHIMMER_W).mul_add(phase, -SHIMMER_W))),
        )
        .into_any_element()
}

pub(super) struct Track {
    chat: TerminalId,
    from: usize,
    to: usize,
    steps: usize,
    chosen: bool,
    reduced: bool,
}

impl EnvironmentShell {
    pub(super) fn effort_track(
        track: &Track,
        facts: &Facts,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let share = level_share(track.to, track.steps);
        let color = level_color(share, pal);
        let low: Hsla = tinted(pal.accent, FILL_LOW_ALPHA).into();
        let (from_w, to_w) = (
            fill_width(track.from, track.steps),
            fill_width(track.to, track.steps),
        );
        let (from_x, to_x) = (
            thumb_left(track.from, track.steps),
            thumb_left(track.to, track.steps),
        );
        let moved = slot(track.from) * STEP_SLOTS + slot(track.to);
        let fill = div()
            .id((FILL_PREFIX, moved))
            .absolute()
            .left_0()
            .top_0()
            .bottom_0()
            .rounded(px(RADIUS_SM))
            .overflow_hidden()
            .bg(linear_gradient(
                GRADIENT_ANGLE,
                linear_color_stop(low, 0.0),
                linear_color_stop(color, 1.0),
            ))
            .when(!track.chosen, |fill| fill.opacity(UNSET_FILL_OPACITY))
            .child(texture(share, pal))
            .when(!track.reduced, |fill| fill.child(shimmer(pal)))
            .with_animation(
                (FILL_PREFIX, moved),
                Animation::new(motion(track.reduced, SLIDE)).with_easing(ease_out_quint()),
                move |fill, delta| fill.w(px((to_w - from_w).mul_add(delta, from_w))),
            );
        let thumb = div()
            .absolute()
            .top(px(THUMB_INSET))
            .bottom(px(THUMB_INSET))
            .w(px(THUMB_W))
            .rounded(px(RADIUS_SM))
            .bg(rgb(pal.bg_surface))
            .shadow(row_shadow(pal, cx))
            .group_hover(THUMB_GROUP, |thumb| {
                thumb
                    .top(px(THUMB_INSET - THUMB_GROW))
                    .bottom(px(THUMB_INSET - THUMB_GROW))
            })
            .with_animation(
                (THUMB_PREFIX, moved),
                Animation::new(motion(track.reduced, SLIDE)).with_easing(snap_bounce),
                move |thumb, delta| thumb.left(px((to_x - from_x).mul_add(delta, from_x))),
            );
        let chat = track.chat;
        let cells = facts.efforts.iter().enumerate().map(|(index, effort)| {
            let effort = *effort;
            div()
                .id((STEP_PREFIX, slot(index)))
                .flex_1()
                .h_full()
                .cursor_pointer()
                .when(index == track.to, |cell| cell.group(THUMB_GROUP))
                .tooltip(move |_, cx| tooltip_view(effort.meaning().into(), cx))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this: &mut Self, _: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        this.grab_effort(chat, effort, cx);
                    }),
                )
                .on_mouse_move(cx.listener(
                    move |this: &mut Self, event: &MouseMoveEvent, _, cx| {
                        let held = event.pressed_button == Some(MouseButton::Left);
                        this.drag_effort(chat, effort, held, cx);
                    },
                ))
        });
        div()
            .id(TRACK_ID)
            .relative()
            .w(px(TRACK_W))
            .h(px(TRACK_H))
            .rounded(px(RADIUS_SM))
            .bg(pal.bg_hover)
            .child(fill)
            .child(thumb)
            .child(div().absolute().inset_0().flex().flex_row().children(cells))
            .into_any_element()
    }

    pub(super) fn effort_header(
        facts: &Facts,
        track: &Track,
        was: Option<Effort>,
        help: bool,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let color = level_color(level_share(track.to, track.steps), pal);
        let now = facts.effort;
        let fade = Animation::new(motion(track.reduced, LEVEL_FADE)).with_easing(ease_out_quint());
        let name = now.map_or(DEFAULT_EFFORT_LABEL, Effort::label);
        let key = now.map_or(STEP_SLOTS, |effort| slot(effort.rank()));
        let leaving = was.filter(|was| Some(*was) != now).map(|was| {
            div()
                .absolute()
                .left_0()
                .top_0()
                .child(was.label())
                .with_animation((LEVEL_OUT_PREFIX, key), fade.clone(), |label, delta| {
                    label.opacity(1.0 - delta)
                })
        });
        let entering =
            div()
                .child(name)
                .with_animation((LEVEL_IN_PREFIX, key), fade, |label, delta| {
                    label.opacity(delta)
                });
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_1_5))
            .text_size(px(TEXT_BASE))
            .line_height(px(LINE_BASE))
            .child(div().text_color(rgb(pal.text_muted)).child(EFFORT_TITLE))
            .child(
                div()
                    .relative()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(color)
                    .children(leaving)
                    .child(entering),
            )
            .child(
                div()
                    .id(HELP_ID)
                    .ml_auto()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(HELP_SIZE))
                    .rounded_full()
                    .border_1()
                    .border_color(pal.divider)
                    .text_size(px(TEXT_XS))
                    .text_color(rgb(if help {
                        pal.text_strong
                    } else {
                        pal.text_muted
                    }))
                    .when(help, |button| button.bg(pal.bg_selected))
                    .cursor_pointer()
                    .hover(|button| button.bg(pal.bg_hover))
                    .active(|button| button.bg(pal.bg_selected))
                    .tooltip(|_, cx| tooltip_view(TOOLTIP_HELP.into(), cx))
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.effort_popover_key(KEY_HELP, cx);
                    }))
                    .child(HELP_GLYPH),
            )
            .into_any_element()
    }

    pub(in crate::environment_shell) fn effort_popover_overlay(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let popover = self
            .effort_popover
            .as_ref()
            .filter(|popover| popover.placement == EffortPlacement::Floating)?;
        let chat = popover.state.chat;
        let reduced = cx.reduce_motion();
        let (facts, track, was) = self.effort_track_of(chat, reduced)?;
        let pal = palette(cx);
        let (root_w, root_h) = self.root_size(window);
        let x = popover.origin.0 - self.frame_inset.0;
        let bottom = root_h - (popover.origin.1 - self.frame_inset.1) + ANCHOR_GAP;
        let left = popover_left(x, root_w);
        let help = popover.state.help.then(|| Self::effort_help(&facts, pal));
        let card = floating_surface(pal, cx)
            .id(CARD_ID)
            .absolute()
            .left(px(left))
            .w(px(POPOVER_W))
            .gap(px(SPACE_3))
            .p(px(CARD_PAD))
            .occlude()
            .track_focus(&popover.focus)
            .on_key_down(cx.listener(|this: &mut Self, event: &KeyDownEvent, _, cx| {
                cx.stop_propagation();
                this.effort_popover_key(event.keystroke.key.as_str(), cx);
            }))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(Self::effort_header(
                &facts,
                &track,
                was,
                popover.state.help,
                pal,
                cx,
            ))
            .children(help)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_between()
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .text_color(rgb(pal.text_subtle))
                    .child(LOW_END)
                    .child(HIGH_END),
            )
            .child(Self::effort_track(&track, &facts, pal, cx))
            .children(facts.fast.map(|on| Self::fast_row(chat, on, pal, cx)))
            .with_animation(
                (OPEN_PREFIX, chat.as_u64()),
                Animation::new(motion(reduced, OPEN)).with_easing(ease_out_quint()),
                move |card, shown| {
                    card.opacity(shown)
                        .bottom(px(RISE.mul_add(shown - 1.0, bottom)))
                },
            );
        let dismiss =
            |this: &mut Self, _: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>| {
                this.close_effort_popover(cx);
            };
        Some(
            div()
                .absolute()
                .inset_0()
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this: &mut Self, _, _, _| this.release_effort()),
                )
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .occlude()
                        .on_mouse_down(MouseButton::Left, cx.listener(dismiss))
                        .on_mouse_down(MouseButton::Right, cx.listener(dismiss)),
                )
                .child(card)
                .into_any_element(),
        )
    }

    pub(super) fn effort_track_of(
        &self,
        chat: TerminalId,
        reduced: bool,
    ) -> Option<(Facts, Track, Option<Effort>)> {
        let facts = self.chat_facts(chat)?;
        let steps = facts.efforts.len();
        if steps < 2 {
            return None;
        }
        let to = current_step(&facts).unwrap_or_default();
        let was = self.effort_was(chat);
        let from = was
            .and_then(|was| facts.efforts.iter().position(|step| *step == was))
            .unwrap_or(to);
        let track = Track {
            chat,
            from,
            to,
            steps,
            chosen: facts.effort.is_some(),
            reduced,
        };
        Some((facts, track, was))
    }

    pub(super) fn effort_help(facts: &Facts, pal: &Palette) -> AnyElement {
        let rows = facts.efforts.iter().map(|effort| {
            let current = facts.effort == Some(*effort);
            div()
                .text_size(px(TEXT_XS))
                .line_height(px(LINE_XS))
                .text_color(rgb(if current {
                    pal.text_strong
                } else {
                    pal.text_muted
                }))
                .when(current, |row| row.font_weight(FontWeight::MEDIUM))
                .child(effort.meaning())
        });
        let title = facts.model.as_ref().map_or_else(
            || HELP_INTRO.to_owned(),
            |model| format!("{HELP_INTRO}: {model}"),
        );
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_1))
            .p(px(SPACE_2))
            .rounded(px(RADIUS_SM))
            .bg(pal.bg_hover)
            .child(
                div()
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(pal.text_default))
                    .child(title),
            )
            .children(rows)
            .into_any_element()
    }

    fn effort_popover_key(&mut self, key: &str, cx: &mut Context<Self>) {
        let Some(chat) = self
            .effort_popover
            .as_ref()
            .map(|popover| popover.state.chat)
        else {
            return;
        };
        let Some(facts) = self.chat_facts(chat) else {
            return;
        };
        let current = facts.effort.or(facts.default_effort);
        let Some(popover) = self.effort_popover.as_mut() else {
            return;
        };
        let step = popover.state.press(key, &facts.efforts, current);
        self.follow_popover(step, cx);
    }

    fn grab_effort(&mut self, chat: TerminalId, effort: Effort, cx: &mut Context<Self>) {
        let current = self.chat_facts(chat).and_then(|facts| facts.effort);
        let Some(popover) = self.effort_popover.as_mut() else {
            return;
        };
        let step = popover.state.grab(effort, current);
        self.follow_popover(step, cx);
    }

    fn drag_effort(
        &mut self,
        chat: TerminalId,
        effort: Effort,
        held: bool,
        cx: &mut Context<Self>,
    ) {
        let dragging = self
            .effort_popover
            .as_ref()
            .is_some_and(|popover| popover.state.dragging);
        if !dragging {
            return;
        }
        let current = self.chat_facts(chat).and_then(|facts| facts.effort);
        let Some(popover) = self.effort_popover.as_ref() else {
            return;
        };
        let step = popover.state.drag_over(effort, current, held);
        self.follow_popover(step, cx);
    }

    pub(super) fn release_effort(&mut self) {
        if let Some(popover) = self.effort_popover.as_mut() {
            popover.state.release();
        }
    }
}
