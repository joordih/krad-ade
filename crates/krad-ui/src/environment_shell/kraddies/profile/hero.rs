use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, pulsating_between, px, rgb, rgba, Animation, AnimationExt as _, AnyElement, Context, Div,
    FontWeight, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled,
};
use krad_domain::KraddieMood;

use super::super::super::EnvironmentShell;
use super::super::layout::{HeroShape, PageLayout};
use super::super::motion::{
    bob, decaying_sway, ease_out_back, IDLE_BOB, IDLE_MAX_FPS, MOOD_POP, MOOD_POP_FROM, WIGGLE,
    WIGGLE_SWAY, WIGGLE_TURNS,
};
use super::super::KraddieEntry;
use crate::components::{chip, Button, ButtonVariant, IconName};
use crate::kraddie_avatar::kraddie_canvas;
use crate::theme::{palette, Palette, RADIUS_XL, SPACE_1, SPACE_1_5, SPACE_2, SPACE_3, SPACE_6};

const BACK_BTN: &str = "kraddie-page-back";
const BACK_LABEL: &str = "All Kraddies";
const WAKE_BTN: &str = "kraddie-page-wake";
const WAKE_LABEL: &str = "Wake up";
const WAKE_TOOLTIP: &str = "Run its manual routine now";
const STUDIO_BTN: &str = "kraddie-page-studio";
const STUDIO_LABEL: &str = "Studio";
const RENAME_BTN: &str = "kraddie-page-rename";
const RENAME_LABEL: &str = "Rename";
const STAGE_ID: &str = "kraddie-stage";
const AVATAR_ID: &str = "kraddie-stage-avatar";
const BOB_ID: &str = "kraddie-stage-bob";
const POP_ID: &str = "kraddie-stage-pop";
const WIGGLE_ID: &str = "kraddie-stage-wiggle";
const PULSE_ID: &str = "kraddie-stage-pulse";
const STAGE_TINT: u32 = 0x14;
const ALPHA_SHIFT: u32 = 8;
const HALO: f32 = 28.0;
const NAME_SIZE: f32 = 24.0;
const NAME_LINE: f32 = 30.0;
const MOOD_DOT: f32 = 8.0;
const PULSE: std::time::Duration = std::time::Duration::from_millis(1400);
const PULSE_LOW: f32 = 0.35;
const PULSE_HIGH: f32 = 1.0;
const ONE_ROUTINE: &str = "1 routine";
const ROUTINES_SUFFIX: &str = "routines";
const RESTING: &str = "Resting";

pub(in crate::environment_shell) const fn mood_line(mood: KraddieMood) -> &'static str {
    match mood {
        KraddieMood::Idle => "Napping between jobs",
        KraddieMood::Working => "Busy at work",
        KraddieMood::Waiting => "Needs a word from you",
        KraddieMood::Done => "Wrapped up and pleased about it",
        KraddieMood::Error => "Tripped over something",
    }
}

pub(in crate::environment_shell) const fn mood_color(mood: KraddieMood, pal: &Palette) -> u32 {
    match mood {
        KraddieMood::Idle => pal.text_muted,
        KraddieMood::Working => pal.info_fg,
        KraddieMood::Waiting => pal.warning_fg,
        KraddieMood::Done => pal.success_fg,
        KraddieMood::Error => pal.error_fg,
    }
}

fn routines_count(count: usize) -> SharedString {
    if count == 1 {
        return ONE_ROUTINE.into();
    }
    format!("{count} {ROUTINES_SUFFIX}").into()
}

fn mood_dot(mood: KraddieMood, pal: &Palette) -> AnyElement {
    let dot = div()
        .flex_none()
        .size(px(MOOD_DOT))
        .rounded_full()
        .bg(rgb(mood_color(mood, pal)));
    if mood != KraddieMood::Working {
        return dot.into_any_element();
    }
    dot.with_animation(
        PULSE_ID,
        Animation::new(PULSE)
            .repeat()
            .with_easing(pulsating_between(PULSE_LOW, PULSE_HIGH)),
        Styled::opacity,
    )
    .into_any_element()
}

impl EnvironmentShell {
    pub(super) fn kraddie_hero(
        &self,
        entry: &KraddieEntry,
        layout: &PageLayout,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let mood = self.kraddie_mood(entry);
        let stacked = layout.hero == HeroShape::Stacked;
        let identity = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .gap(px(SPACE_1_5))
            .when(stacked, Styled::items_center)
            .child(
                div()
                    .truncate()
                    .text_size(px(NAME_SIZE))
                    .line_height(px(NAME_LINE))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(pal.text_strong))
                    .child(entry.kraddie.name.to_string()),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(SPACE_1_5))
                    .text_color(rgb(pal.text_default))
                    .child(mood_dot(mood, pal))
                    .child(mood_line(mood)),
            )
            .child(self.hero_facts(entry, pal));
        let stage = div()
            .id(STAGE_ID)
            .flex()
            .gap(px(SPACE_6))
            .p(px(SPACE_6))
            .rounded(px(RADIUS_XL))
            .bg(rgba((pal.accent << ALPHA_SHIFT) | STAGE_TINT))
            .border_1()
            .border_color(pal.divider)
            .map(|stage| {
                if stacked {
                    stage.flex_col().items_center()
                } else {
                    stage.flex_row().items_center()
                }
            })
            .child(self.hero_avatar(entry, mood, layout.avatar, cx))
            .child(identity)
            .child(Self::hero_actions(entry, stacked, cx));
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_2))
            .child(
                div().flex().flex_row().child(
                    Button::new(BACK_BTN)
                        .label(BACK_LABEL)
                        .icon(IconName::ChevronLeft)
                        .small()
                        .on_click(
                            cx.listener(|this: &mut Self, _, _, cx| this.close_kraddie_page(cx)),
                        ),
                ),
            )
            .child(stage)
            .into_any_element()
    }

    fn hero_facts(&self, entry: &KraddieEntry, pal: &Palette) -> Div {
        let harness = self.settings.acp.harness_name(entry.kraddie.harness);
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(px(SPACE_1))
            .child(chip(harness.into_owned(), pal))
            .child(chip(routines_count(entry.automations.len()), pal))
            .when(!entry.kraddie.switch.is_on(), |facts| {
                facts.child(chip(RESTING, pal))
            })
    }

    fn hero_avatar(
        &self,
        entry: &KraddieEntry,
        mood: KraddieMood,
        side: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let sprite = entry.sprite(mood);
        self.kraddies.note_drawn(sprite);
        let key = entry.kraddie.id().to_string();
        let pokes = self.kraddies.profile.pokes;
        let popped = div()
            .size(px(side))
            .child(kraddie_canvas(sprite, self.kraddies.frame()))
            .with_animation(
                (SharedString::from(format!("{POP_ID}-{key}")), mood.index()),
                Animation::new(MOOD_POP),
                move |avatar, t| {
                    avatar.size(px(
                        side * (MOOD_POP_FROM + (1.0 - MOOD_POP_FROM) * ease_out_back(t))
                    ))
                },
            );
        let swaying = div().relative().child(popped);
        let swaying = if pokes == 0 {
            swaying.into_any_element()
        } else {
            swaying
                .with_animation(
                    (SharedString::from(format!("{WIGGLE_ID}-{key}")), pokes),
                    Animation::new(WIGGLE),
                    |avatar, t| avatar.left(px(decaying_sway(t, WIGGLE_SWAY, WIGGLE_TURNS))),
                )
                .into_any_element()
        };
        let bobbing = div().relative().child(swaying).with_animation(
            BOB_ID,
            Animation::new(IDLE_BOB).repeat().with_max_fps(IDLE_MAX_FPS),
            |avatar, t| avatar.top(px(bob(t))),
        );
        div()
            .id(AVATAR_ID)
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(side + HALO))
            .rounded_full()
            .bg(rgb(pal.bg_surface))
            .border_1()
            .border_color(pal.divider)
            .cursor_pointer()
            .on_hover(cx.listener(|this: &mut Self, hovered: &bool, _, cx| {
                if *hovered {
                    this.kraddies.profile.pokes = this.kraddies.profile.pokes.wrapping_add(1);
                    cx.notify();
                }
            }))
            .child(bobbing)
            .into_any_element()
    }

    fn hero_actions(entry: &KraddieEntry, stacked: bool, cx: &mut Context<Self>) -> AnyElement {
        let id = entry.kraddie.id();
        let (wake, studio, rename) = (id.clone(), id.clone(), id);
        div()
            .flex()
            .flex_none()
            .flex_wrap()
            .gap(px(SPACE_2))
            .map(|actions| {
                if stacked {
                    actions.flex_row().justify_center()
                } else {
                    actions.flex_col().items_end().gap(px(SPACE_3))
                }
            })
            .child(
                Button::new(WAKE_BTN)
                    .label(WAKE_LABEL)
                    .icon(IconName::Play)
                    .variant(ButtonVariant::Primary)
                    .tooltip(WAKE_TOOLTIP)
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.run_kraddie_now(&wake, cx);
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(SPACE_1))
                    .child(
                        Button::new(STUDIO_BTN)
                            .label(STUDIO_LABEL)
                            .icon(IconName::Pencil)
                            .small()
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                this.open_kraddie_studio(&studio, cx);
                            })),
                    )
                    .child(
                        Button::new(RENAME_BTN)
                            .label(RENAME_LABEL)
                            .icon(IconName::Settings)
                            .small()
                            .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                                this.open_kraddie_editor(Some(&rename), window, cx);
                            })),
                    ),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mood_has_its_own_line_and_counts_read_naturally() {
        let mut lines: Vec<&str> = KraddieMood::ALL.map(mood_line).to_vec();
        lines.sort_unstable();
        lines.dedup();
        assert_eq!(lines.len(), KraddieMood::ALL.len());
        assert_eq!(routines_count(1).as_ref(), ONE_ROUTINE);
        assert_eq!(routines_count(3).as_ref(), "3 routines");
        assert_eq!(routines_count(0).as_ref(), "0 routines");
    }
}
