use gpui_kit::{
    div, px, rgb, AnyElement, Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use krad_domain::{model_traits, ModelTraits, Score, TerminalId};

use super::super::marks::Mark;
use super::super::{Choosing, ModelChoice, PickFor};
use super::{small_text, META_SEPARATOR, TOOLTIP_FAVORITE};
use crate::components::{harness_icon, icon, tooltip_view, IconName};
use crate::environment_shell::surface::SurfaceCommand;
use crate::environment_shell::EnvironmentShell;
use crate::theme::{
    Palette, ICON_SM, LINE_BASE, LINE_SM, LINE_XS, SPACE_1, SPACE_1_5, SPACE_3, TEXT_BASE, TEXT_SM,
    TEXT_XS,
};

const TILE: f32 = 40.0;
const TILE_RADIUS: f32 = 11.0;
const PIP_W: f32 = 8.0;
const PIP_H: f32 = 5.0;
const PIP_GAP: f32 = 2.0;
const PIP_RADIUS: f32 = 2.0;
const SWITCH_W: f32 = 30.0;
const SWITCH_H: f32 = 18.0;
const KNOB: f32 = 14.0;
const KNOB_INSET: f32 = 2.0;
const DETAIL_STAR_ID: &str = "model-deck-detail-star";
const THINKING_ID: &str = "model-deck-thinking";
const SMART_LABEL: &str = "Smart";
const SPEED_LABEL: &str = "Speed";
const COST_LABEL: &str = "Cost";
const EFFORT_LABEL: &str = "Effort";
const THINKING_LABEL: &str = "Extended thinking";
const NOT_ADJUSTABLE: &str = "Not adjustable";
const ONLY_SUFFIX: &str = "only";
const PICK_TO_TUNE: &str = "Pick this model to tune its effort.";
const CONTEXT_SUFFIX: &str = "context";

fn pips(score: Score, pal: &Palette) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .gap(px(PIP_GAP))
        .children((1..=Score::MAX).map(|step| {
            let pip = div().w(px(PIP_W)).h(px(PIP_H)).rounded(px(PIP_RADIUS));
            if step <= score.value() {
                pip.bg(rgb(pal.text_default))
            } else {
                pip.bg(pal.divider)
            }
        }))
        .into_any_element()
}

fn meter(label: &'static str, score: Score, pal: &Palette) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(SPACE_1_5))
        .child(
            div()
                .text_size(px(TEXT_XS))
                .line_height(px(LINE_XS))
                .text_color(rgb(pal.text_subtle))
                .child(label),
        )
        .child(pips(score, pal))
        .into_any_element()
}

fn meters(traits: ModelTraits, pal: &Palette) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .gap(px(SPACE_3))
        .child(meter(SMART_LABEL, traits.smart, pal))
        .child(meter(SPEED_LABEL, traits.speed, pal))
        .child(meter(COST_LABEL, traits.cost, pal))
        .into_any_element()
}

fn switch(on: bool, pal: &Palette) -> AnyElement {
    let left = if on {
        SWITCH_W - KNOB - KNOB_INSET
    } else {
        KNOB_INSET
    };
    let track = div()
        .relative()
        .flex_none()
        .w(px(SWITCH_W))
        .h(px(SWITCH_H))
        .rounded_full();
    if on {
        track.bg(rgb(pal.accent))
    } else {
        track.bg(pal.bg_selected)
    }
    .child(
        div()
            .absolute()
            .top(px(KNOB_INSET))
            .left(px(left))
            .size(px(KNOB))
            .rounded_full()
            .bg(rgb(pal.on_accent)),
    )
    .into_any_element()
}

impl EnvironmentShell {
    fn detail_star(
        index: usize,
        choice: &ModelChoice,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        choice
            .id
            .is_some()
            .then(|| {
                div()
                    .id(DETAIL_STAR_ID)
                    .flex_none()
                    .cursor_pointer()
                    .tooltip(|_, cx| tooltip_view(TOOLTIP_FAVORITE.into(), cx))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.toggle_model_mark(index, Mark::Favorite, cx);
                    }))
                    .child(icon(
                        if choice.marks.favorite {
                            IconName::StarFilled
                        } else {
                            IconName::Star
                        },
                        ICON_SM,
                        if choice.marks.favorite {
                            pal.warning_fg
                        } else {
                            pal.text_subtle
                        },
                    ))
            })
            .map(IntoElement::into_any_element)
    }

    pub(super) fn deck_detail(
        &self,
        choosing: &Choosing,
        target: PickFor,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let entries = choosing.entries();
        let Some((harness, choice)) = entries.get(choosing.highlighted).copied() else {
            return div().into_any_element();
        };
        let index = choosing.highlighted;
        let (glyph, color) = harness_icon(harness, pal);
        let group_name = choosing
            .groups
            .iter()
            .find(|group| group.harness == harness)
            .map_or_else(|| harness.display_name().into(), |group| group.name.clone());
        let meta = choice.context.map_or_else(
            || group_name.to_string(),
            |context| {
                format!(
                    "{group_name}{META_SEPARATOR}{} {CONTEXT_SUFFIX}",
                    context.label()
                )
            },
        );
        let star = Self::detail_star(index, choice, pal, cx);
        let top = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_3))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(px(TILE))
                    .rounded(px(TILE_RADIUS))
                    .bg(pal.bg_hover)
                    .border_1()
                    .border_color(pal.divider)
                    .child(icon(glyph, ICON_SM, color)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(TEXT_BASE))
                            .line_height(px(LINE_BASE))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(pal.text_strong))
                            .child(choice.label.clone()),
                    )
                    .child(small_text(meta, pal.text_muted)),
            )
            .children(star);
        let traits = choice.id.as_ref().and_then(model_traits);
        let chat = match target {
            PickFor::Chat(chat) => Some(chat),
            PickFor::Docs => None,
        };
        let config = chat.map(|chat| self.deck_config(chat, choice.current, pal, cx));
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_3))
            .p(px(SPACE_3 + SPACE_1))
            .child(top)
            .children(choice.description.clone().map(|text| {
                div()
                    .text_size(px(TEXT_SM))
                    .line_height(px(LINE_SM))
                    .text_color(rgb(pal.text_muted))
                    .child(text)
            }))
            .children(traits.map(|traits| meters(traits, pal)))
            .children(config.map(|config| {
                div()
                    .pt(px(SPACE_3))
                    .border_t_1()
                    .border_color(pal.divider)
                    .child(config)
            }))
            .into_any_element()
    }

    fn deck_config(
        &self,
        chat: TerminalId,
        current: bool,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !current {
            return small_text(PICK_TO_TUNE, pal.text_subtle);
        }
        let tuning = self.chat_tuning(chat);
        let effort = self.embedded_effort(chat, cx).unwrap_or_else(|| {
            let fixed = tuning.and_then(|tuning| tuning.fixed_effort).map_or_else(
                || NOT_ADJUSTABLE.to_owned(),
                |effort| format!("{} {ONLY_SUFFIX}", effort.label()),
            );
            div()
                .flex()
                .flex_row()
                .justify_between()
                .text_size(px(TEXT_SM))
                .line_height(px(LINE_SM))
                .child(div().text_color(rgb(pal.text_default)).child(EFFORT_LABEL))
                .child(div().text_color(rgb(pal.text_muted)).child(fixed))
                .into_any_element()
        });
        let thinking = tuning.and_then(|tuning| tuning.thinking).map(|on| {
            div()
                .id(THINKING_ID)
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .cursor_pointer()
                .text_size(px(TEXT_SM))
                .line_height(px(LINE_SM))
                .text_color(rgb(pal.text_default))
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.command_surface(chat, SurfaceCommand::ToggleThinking, cx);
                }))
                .child(THINKING_LABEL)
                .child(switch(on, pal))
        });
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_3))
            .child(effort)
            .children(thinking)
            .into_any_element()
    }
}
