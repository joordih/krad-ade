use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, Animation, AnimationExt as _, AnyElement, Context, Div, ElementId, FontWeight,
    Hsla, InteractiveElement, IntoElement, ParentElement, SharedString, StatefulInteractiveElement,
    Styled,
};
use krad_domain::{AutomationKey, KraddieMood, Switch};

use super::super::EnvironmentShell;
use super::model::{AutomationCard, Duty};
use super::wording::{countdown, elapsed, Tone};
use crate::components::motion::{fade_in, idle_bob, PULSE_FPS, PULSE_PERIOD};
use crate::components::{icon, toggle_switch, Button, IconName};
use crate::kraddie_avatar::kraddie_avatar;
use crate::theme::{
    level_fill, row_fill, row_shadow, tinted, Palette, SurfaceLevel, ICON_SM, LINE_SM, LINE_XS,
    RADIUS_LG, RADIUS_MD, SPACE_0_5, SPACE_1, SPACE_1_5, SPACE_2, SPACE_3, TEXT_SM, TEXT_XS,
};

const CARD_PREFIX: &str = "automation-card";
const ROW_PREFIX: &str = "automation-row";
const SWITCH_PREFIX: &str = "automation-card-switch";
const SWITCH_ANIM_PREFIX: &str = "automation-card-knob";
const RUN_PREFIX: &str = "automation-card-run";
const PULSE_PREFIX: &str = "automation-card-pulse";
const COUNTDOWN_PREFIX: &str = "automation-card-countdown";
const CARD_AVATAR: f32 = 34.0;
const ROW_AVATAR: f32 = 24.0;
const DOT: f32 = 7.0;
const PULSE_REACH: f32 = 2.6;
const PULSE_ALPHA: f32 = 0.55;
const PRESSED_OPACITY: f32 = 0.85;
const HOVER_RING_ALPHA: u32 = 0x55;
const KEPT_BY: &str = "kept by";
const NEVER_RAN: &str = "No runs yet";
const WAITING: &str = "waits for its cue";
const RESTING: &str = "resting";
const OWNER_RESTING: &str = "its Kraddie is resting";
const RUN_TOOLTIP: &str = "Run it now";
const ON_TOOLTIP: &str = "On duty: click to let it rest";
const OFF_TOOLTIP: &str = "Resting: click to put it on duty";
const AGE_SEPARATOR: &str = " · ";

pub(super) const fn tone_color(tone: Tone, pal: &Palette) -> u32 {
    match tone {
        Tone::Live => pal.info_fg,
        Tone::Good => pal.success_fg,
        Tone::Attention => pal.warning_fg,
        Tone::Bad => pal.error_fg,
        Tone::Quiet => pal.text_disabled,
    }
}

fn status_dot(id: ElementId, color: u32, live: bool) -> Div {
    let reach = DOT * PULSE_REACH;
    let dot = div().size(px(DOT)).rounded_full().bg(rgb(color));
    let wrapper = div()
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(DOT));
    if !live {
        return wrapper.child(dot);
    }
    let ring = div()
        .absolute()
        .rounded_full()
        .bg(rgb(color))
        .with_animation(
            id,
            Animation::new(PULSE_PERIOD)
                .repeat()
                .with_max_fps(PULSE_FPS),
            move |ring, t| {
                let side = DOT + (reach - DOT) * t;
                let offset = (DOT - side) / 2.0;
                ring.size(px(side))
                    .left(px(offset))
                    .top(px(offset))
                    .opacity(PULSE_ALPHA * (1.0 - t))
            },
        );
    wrapper.child(ring).child(dot)
}

fn next_text(card: &AutomationCard) -> SharedString {
    if card.duty == Duty::OwnerResting {
        return OWNER_RESTING.into();
    }
    if !card.on_duty() {
        return RESTING.into();
    }
    card.next_in
        .map_or_else(|| WAITING.into(), |next| countdown(next).into())
}

fn last_text(card: &AutomationCard) -> SharedString {
    match (&card.last_label, card.last.and_then(|last| last.age)) {
        (Some(label), Some(age)) => format!("{label}{AGE_SEPARATOR}{}", elapsed(age)).into(),
        (Some(label), None) => label.clone().into(),
        (None, _) => NEVER_RAN.into(),
    }
}

fn caption(text: impl Into<SharedString>, color: u32) -> Div {
    div()
        .min_w_0()
        .truncate()
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .text_color(rgb(color))
        .child(text.into())
}

impl EnvironmentShell {
    fn card_avatar(
        &self,
        card: &AutomationCard,
        side: f32,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let kraddie = self.kraddies.entry(&card.key.kraddie)?;
        let mood = if card.on_duty() {
            self.kraddie_mood(kraddie)
        } else {
            KraddieMood::Idle
        };
        let sprite = kraddie.sprite(mood);
        self.kraddies.note_drawn(sprite);
        let frame = self.kraddies.frame();
        let bob = if card.on_duty() && !cx.reduce_motion() {
            idle_bob(frame)
        } else {
            0.0
        };
        Some(
            div()
                .relative()
                .flex_none()
                .top(px(bob))
                .child(kraddie_avatar(sprite, side, frame))
                .into_any_element(),
        )
    }

    fn duty_switch(
        index: usize,
        card: &AutomationCard,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = card.key.clone();
        let on = card.switched_on();
        div()
            .id((SWITCH_PREFIX, index))
            .flex_none()
            .cursor_pointer()
            .tooltip(move |_, cx| {
                crate::components::tooltip_view(
                    if on { ON_TOOLTIP } else { OFF_TOOLTIP }.into(),
                    cx,
                )
            })
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                cx.stop_propagation();
                this.flip_automation(&key, cx);
            }))
            .child(toggle_switch((SWITCH_ANIM_PREFIX, index), on, pal))
            .into_any_element()
    }

    fn flip_automation(&mut self, key: &AutomationKey, cx: &mut Context<Self>) {
        let Some((_, entry)) = self.kraddies.automation(key) else {
            return;
        };
        let switch = match entry.automation.switch {
            Switch::On => Switch::Off,
            Switch::Off => Switch::On,
        };
        self.switch_automation(key, switch, cx);
        if let Some(editor) = self
            .automations
            .editor
            .as_mut()
            .filter(|editor| editor.editing() == Some(key))
        {
            editor.set_switch(switch);
        }
    }

    fn run_button(index: usize, key: &AutomationKey, cx: &mut Context<Self>) -> Button {
        let key = key.clone();
        Button::new((RUN_PREFIX, index))
            .icon(IconName::Play)
            .small()
            .tooltip(RUN_TOOLTIP)
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                cx.stop_propagation();
                this.run_automation_now(&key, cx);
            }))
    }

    fn card_footer(
        index: usize,
        card: &AutomationCard,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> Div {
        let tone = card.last.map_or(Tone::Quiet, |last| last.tone);
        let next = next_text(card);
        let ticking = card.on_duty() && card.next_in.is_some();
        let countdown_key = card.next_in.map_or(0, |next| next.as_secs());
        let next_line = if ticking {
            fade_in(
                ElementId::NamedInteger(COUNTDOWN_PREFIX.into(), countdown_key),
                caption(next, pal.text_strong).font_weight(FontWeight::MEDIUM),
            )
        } else {
            caption(next, pal.text_muted).into_any_element()
        };
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_1_5))
            .pt(px(SPACE_2))
            .border_t_1()
            .border_color(pal.divider)
            .child(status_dot(
                ElementId::NamedInteger(
                    PULSE_PREFIX.into(),
                    u64::try_from(index).unwrap_or_default(),
                ),
                tone_color(tone, pal),
                card.running,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(caption(last_text(card), pal.text_muted)),
            )
            .child(next_line)
            .child(Self::run_button(index, &card.key, cx))
    }

    pub(super) fn automation_card(
        &self,
        index: usize,
        card: &AutomationCard,
        chosen: bool,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = card.key.clone();
        let header = div()
            .flex()
            .flex_row()
            .items_start()
            .gap(px(SPACE_2))
            .children(self.card_avatar(card, CARD_AVATAR, cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(px(SPACE_0_5))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(TEXT_SM))
                            .line_height(px(LINE_SM))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(pal.text_strong))
                            .child(SharedString::from(card.name.clone())),
                    )
                    .child(caption(format!("{KEPT_BY} {}", card.owner), pal.text_muted)),
            )
            .child(Self::duty_switch(index, card, pal, cx));
        let cue = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_1_5))
            .px(px(SPACE_2))
            .py(px(SPACE_1))
            .rounded(px(RADIUS_MD))
            .bg(pal.bg_hover)
            .child(icon(IconName::Repeat, ICON_SM, pal.text_subtle))
            .child(caption(card.cues.clone(), pal.text_default));
        let footer = Self::card_footer(index, card, pal, cx);
        let accent = pal.accent;
        let ring: Hsla = if chosen {
            tinted(accent, HOVER_RING_ALPHA).into()
        } else {
            pal.divider.into()
        };
        div()
            .id((CARD_PREFIX, index))
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .overflow_hidden()
            .gap(px(SPACE_3))
            .p(px(SPACE_3))
            .rounded(px(RADIUS_LG))
            .border_1()
            .border_color(ring)
            .bg(level_fill(SurfaceLevel::Pane, pal.bg_panel, cx))
            .cursor_pointer()
            .when(!card.on_duty(), |tile| tile.opacity(PRESSED_OPACITY))
            .hover(move |style| style.border_color(tinted(accent, HOVER_RING_ALPHA)))
            .active(|style| style.opacity(PRESSED_OPACITY))
            .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                this.open_automation_editor(&key, window, cx);
            }))
            .child(header)
            .child(cue)
            .child(footer)
            .into_any_element()
    }

    pub(super) fn automation_row(
        &self,
        index: usize,
        card: &AutomationCard,
        chosen: bool,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = card.key.clone();
        let tone = card.last.map_or(Tone::Quiet, |last| last.tone);
        div()
            .id((ROW_PREFIX, index))
            .flex()
            .flex_1()
            .min_w_0()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .p(px(SPACE_2))
            .rounded(px(RADIUS_MD))
            .cursor_pointer()
            .when(chosen, |row| {
                row.bg(row_fill(pal, cx)).shadow(row_shadow(pal, cx))
            })
            .when(!chosen, |row| row.hover(|style| style.bg(pal.bg_hover)))
            .active(|style| style.opacity(PRESSED_OPACITY))
            .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                this.open_automation_editor(&key, window, cx);
            }))
            .children(self.card_avatar(card, ROW_AVATAR, cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(px(SPACE_0_5))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(TEXT_SM))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(pal.text_strong))
                            .child(SharedString::from(card.name.clone())),
                    )
                    .child(caption(card.cues.clone(), pal.text_muted)),
            )
            .child(caption(next_text(card), pal.text_subtle).flex_none())
            .child(status_dot(
                ElementId::NamedInteger(
                    PULSE_PREFIX.into(),
                    u64::try_from(index).unwrap_or_default(),
                ),
                tone_color(tone, pal),
                card.running,
            ))
            .into_any_element()
    }
}
