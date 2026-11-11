use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, point, px, rgb, size, AnyElement, Context, FontWeight, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, ParentElement, SharedString, StatefulInteractiveElement, Styled,
    Window,
};
use krad_chat::ModelSource;

use super::super::EnvironmentShell;
use super::layout::picker_layout;
use super::marks::Mark;
use super::{Choosing, ModelChoice, Shelf};
use crate::components::floating::{floating_at, floating_surface, place, Anchor, Side};
use crate::components::{harness_icon, icon, tooltip_view, IconName};
use crate::theme::{
    palette, Palette, ICON, ICON_SM, LINE_BASE, LINE_XS, SPACE_1, SPACE_1_5, SPACE_2, SPACE_3,
    TEXT_BASE, TEXT_SM, TEXT_XS,
};

const PICKER_H: f32 = 380.0;
const RAIL_W: f32 = 48.0;
const RAIL_BUTTON: f32 = 34.0;
const SEARCH_H: f32 = 44.0;
const ROW_H: f32 = 32.0;
const DESCRIBED_ROW_H: f32 = 46.0;
const BADGE_H: f32 = 16.0;
const BADGE_RADIUS: f32 = 5.0;
const RECOMMENDED_BADGE: &str = "Recommended";
const DEFAULT_BADGE: &str = "Default";
const MISSING_BADGE: &str = "Unavailable";
const BUTTON_RADIUS: f32 = 9.0;
const CARET_W: f32 = 1.5;
const CARET_H: f32 = 18.0;
const ANCHOR_GAP: f32 = 8.0;
const PICKER_ID: &str = "model-picker";
const PICKER_MOTION_ID: &str = "model-picker-motion";
const LIST_ID: &str = "model-picker-list";
const RAIL_PREFIX: &str = "model-picker-harness";
const ROW_PREFIX: &str = "model-picker-row";
const SEARCH_PLACEHOLDER: &str = "Search models";
const NO_MODELS_TEXT: &str = "No models";
const DETECTING_AGENTS_TEXT: &str = "Detecting agents on";
const SECTION_H: f32 = 26.0;
const FREE_LABEL: &str = "Free";
const FAVORITES_LABEL: &str = "Favorites";
const MODELS_LABEL: &str = "Models";
const STAR_PREFIX: &str = "model-picker-star";
const DEFAULT_PREFIX: &str = "model-picker-default";
const TOOLTIP_ADD_FAVORITE: &str = "Add to favorites";
const TOOLTIP_REMOVE_FAVORITE: &str = "Remove from favorites";
const TOOLTIP_SET_DEFAULT: &str = "Use for new chats of this agent";
const TOOLTIP_CLEAR_DEFAULT: &str = "Stop using for new chats";
const MARK_BUTTON: f32 = 22.0;
const MARK_RADIUS: f32 = 6.0;

fn section_label(section: &ModelSource) -> &str {
    match section {
        ModelSource::Plan { name, .. } | ModelSource::Provider(name) => name,
        ModelSource::Free => FREE_LABEL,
    }
}

fn shelf_label(shelf: Shelf<'_>, follows: bool) -> Option<&str> {
    match shelf {
        Shelf::Favorites => Some(FAVORITES_LABEL),
        Shelf::Source(Some(section)) => Some(section_label(section)),
        Shelf::Source(None) => follows.then_some(MODELS_LABEL),
    }
}

fn section_header(label: &str, pal: &Palette) -> AnyElement {
    div()
        .flex_none()
        .flex()
        .items_end()
        .h(px(SECTION_H))
        .px(px(SPACE_2))
        .pb(px(SPACE_1 / 2.0))
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(pal.text_subtle))
        .child(label.to_owned())
        .into_any_element()
}

fn badge(text: &'static str, fill: u32, color: u32) -> AnyElement {
    div()
        .flex_none()
        .flex()
        .items_center()
        .h(px(BADGE_H))
        .px(px(SPACE_1_5))
        .rounded(px(BADGE_RADIUS))
        .bg(rgb(fill))
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .text_color(rgb(color))
        .child(text)
        .into_any_element()
}

fn badges(choice: &ModelChoice, pal: &Palette) -> impl Iterator<Item = AnyElement> {
    let default = choice
        .marks
        .default
        .then(|| badge(DEFAULT_BADGE, pal.chip_bg, pal.text_strong));
    let recommended = choice
        .recommended
        .then(|| badge(RECOMMENDED_BADGE, pal.chip_bg, pal.chip_fg));
    let context = choice
        .context
        .map(|context| badge(context.label(), pal.chip_bg, pal.chip_fg));
    let missing = choice
        .missing
        .then(|| badge(MISSING_BADGE, pal.chip_bg, pal.warning_fg));
    default
        .into_iter()
        .chain(recommended)
        .chain(context)
        .chain(missing)
}

fn choice_text(choice: &ModelChoice, described: bool, pal: &Palette) -> AnyElement {
    let name = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(SPACE_1_5))
        .min_w_0()
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_size(px(TEXT_BASE))
                .line_height(px(LINE_BASE))
                .text_color(rgb(pal.text_strong))
                .child(choice.label.clone()),
        )
        .children(badges(choice, pal));
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .child(name)
        .children(
            choice
                .description
                .as_ref()
                .filter(|_| described)
                .map(|text| {
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(px(TEXT_XS))
                        .line_height(px(LINE_XS))
                        .text_color(rgb(pal.text_muted))
                        .child(text.clone())
                }),
        )
        .into_any_element()
}

fn element_index(index: usize) -> u64 {
    u64::try_from(index).unwrap_or(u64::MAX)
}

fn search_row(query: &str, pal: &Palette) -> AnyElement {
    div()
        .flex_none()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(SPACE_3))
        .h(px(SEARCH_H))
        .px(px(SPACE_3))
        .border_b_1()
        .border_color(pal.divider)
        .text_size(px(TEXT_BASE))
        .line_height(px(LINE_BASE))
        .child(icon(IconName::Search, ICON, pal.text_muted))
        .when(!query.is_empty(), |row| {
            row.child(
                div()
                    .text_color(rgb(pal.text_strong))
                    .child(query.to_owned()),
            )
        })
        .child(
            div()
                .flex_none()
                .w(px(CARET_W))
                .h(px(CARET_H))
                .bg(rgb(pal.text_strong)),
        )
        .when(query.is_empty(), |row| {
            row.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(pal.text_subtle))
                    .child(SEARCH_PLACEHOLDER),
            )
        })
        .into_any_element()
}

impl EnvironmentShell {
    fn picker_rail(choosing: &Choosing, pal: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let buttons: Vec<AnyElement> = choosing
            .groups
            .iter()
            .enumerate()
            .map(|(index, group)| {
                let harness = group.harness;
                let name = group.name.clone();
                let selected = harness == choosing.harness;
                let (glyph, color) = harness_icon(harness, pal);
                div()
                    .id((RAIL_PREFIX, element_index(index)))
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(RAIL_BUTTON))
                    .rounded(px(BUTTON_RADIUS))
                    .cursor_pointer()
                    .when(selected, |button| button.bg(pal.bg_selected))
                    .when(!selected, |button| {
                        button.hover(|style| style.bg(pal.bg_hover))
                    })
                    .tooltip(move |_, cx| tooltip_view(name.clone(), cx))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.show_picker_harness(harness, cx);
                    }))
                    .child(icon(glyph, ICON, color))
                    .into_any_element()
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .flex_none()
            .items_center()
            .gap(px(SPACE_1_5))
            .w(px(RAIL_W))
            .py(px(SPACE_2))
            .border_r_1()
            .border_color(pal.divider)
            .children(buttons)
            .into_any_element()
    }

    fn mark_button(
        id: (&'static str, u64),
        glyph: IconName,
        color: u32,
        tooltip: &'static str,
        on_click: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(id)
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(MARK_BUTTON))
            .rounded(px(MARK_RADIUS))
            .cursor_pointer()
            .hover(|style| style.bg(pal.bg_selected))
            .tooltip(move |_, cx| tooltip_view(tooltip.into(), cx))
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                cx.stop_propagation();
                on_click(this, cx);
            }))
            .child(icon(glyph, ICON_SM, color))
            .into_any_element()
    }

    fn mark_buttons(
        choice: &ModelChoice,
        index: usize,
        highlighted: bool,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        if choice.id.is_none() {
            return Vec::new();
        }
        let at = element_index(index);
        let default = (highlighted || choice.marks.default).then(|| {
            let (color, tooltip) = if choice.marks.default {
                (pal.accent, TOOLTIP_CLEAR_DEFAULT)
            } else {
                (pal.text_subtle, TOOLTIP_SET_DEFAULT)
            };
            Self::mark_button(
                (DEFAULT_PREFIX, at),
                IconName::Target,
                color,
                tooltip,
                move |this, cx| this.toggle_model_mark(index, Mark::NewChatDefault, cx),
                pal,
                cx,
            )
        });
        let star = (highlighted || choice.marks.favorite).then(|| {
            let (glyph, color, tooltip) = if choice.marks.favorite {
                (IconName::StarFilled, pal.accent, TOOLTIP_REMOVE_FAVORITE)
            } else {
                (IconName::Star, pal.text_subtle, TOOLTIP_ADD_FAVORITE)
            };
            Self::mark_button(
                (STAR_PREFIX, at),
                glyph,
                color,
                tooltip,
                move |this, cx| this.toggle_model_mark(index, Mark::Favorite, cx),
                pal,
                cx,
            )
        });
        default.into_iter().chain(star).collect()
    }

    fn picker_empty_text(&self) -> SharedString {
        if self.host_agents_known(&self.active) {
            return NO_MODELS_TEXT.into();
        }
        self.envs.get(&self.active).map_or_else(
            || NO_MODELS_TEXT.into(),
            |live| format!("{DETECTING_AGENTS_TEXT} {}", live.data.host).into(),
        )
    }

    fn picker_rows(
        choosing: &Choosing,
        described: bool,
        empty_text: SharedString,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let shown = choosing.shown();
        let mut rows: Vec<AnyElement> = Vec::with_capacity(shown.len());
        let mut shelf = None;
        for (index, choice) in shown.iter().enumerate() {
            let current = choice.shelf();
            if shelf != Some(current) {
                if let Some(label) = shelf_label(current, shelf.is_some()) {
                    rows.push(section_header(label, pal));
                }
            }
            shelf = Some(current);
            let highlighted = index == choosing.highlighted;
            let tall = described && choice.description.is_some();
            rows.push(
                div()
                    .id((ROW_PREFIX, element_index(index)))
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(SPACE_2))
                    .h(px(if tall { DESCRIBED_ROW_H } else { ROW_H }))
                    .px(px(SPACE_2))
                    .rounded(px(BUTTON_RADIUS))
                    .cursor_pointer()
                    .when(highlighted, |row| row.bg(pal.bg_hover))
                    .active(|row| row.bg(pal.bg_selected))
                    .on_hover(cx.listener(move |this: &mut Self, hovered: &bool, _, cx| {
                        if *hovered {
                            this.highlight_model(index, cx);
                        }
                    }))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.pick_model(index, cx);
                    }))
                    .child(choice_text(choice, described, pal))
                    .children(Self::mark_buttons(choice, index, highlighted, pal, cx))
                    .when(choice.current, |row| {
                        row.child(icon(IconName::Tick, ICON_SM, pal.text_strong))
                    })
                    .into_any_element(),
            );
        }
        let empty = shown.is_empty().then(|| {
            div()
                .px(px(SPACE_2))
                .py(px(SPACE_2))
                .text_size(px(TEXT_SM))
                .text_color(rgb(pal.text_muted))
                .child(empty_text)
        });
        div()
            .id(LIST_ID)
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(px(SPACE_1 / 2.0))
            .p(px(SPACE_1_5))
            .overflow_y_scroll()
            .children(rows)
            .children(empty)
            .into_any_element()
    }

    pub(in crate::environment_shell) fn model_picker_overlay(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let picker = self.model_picker.as_ref()?;
        let pal = palette(cx);
        let (root_w, root_h) = self.root_size(window);
        let (x, y) = (
            picker.origin.0 - self.frame_inset.0,
            picker.origin.1 - self.frame_inset.1,
        );
        let layout = picker_layout(root_w);
        let placed = place(
            Anchor::Point(point(px(x), px(y - ANCHOR_GAP))),
            size(px(layout.width), px(PICKER_H)),
            size(px(root_w), px(root_h)),
            Side::Above,
        );
        let dismiss = |this: &mut Self,
                       _: &gpui_kit::MouseDownEvent,
                       _: &mut Window,
                       cx: &mut Context<Self>| {
            this.close_model_picker(cx);
        };
        Some(
            div()
                .absolute()
                .inset_0()
                .track_focus(&picker.focus)
                .on_key_down(
                    cx.listener(|this: &mut Self, event: &KeyDownEvent, window, cx| {
                        this.on_model_picker_key(event, window.capslock(), cx);
                    }),
                )
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .occlude()
                        .on_mouse_down(MouseButton::Left, cx.listener(dismiss))
                        .on_mouse_down(MouseButton::Right, cx.listener(dismiss)),
                )
                .child(floating_at(
                    PICKER_MOTION_ID,
                    placed,
                    floating_surface(pal, cx)
                        .id(PICKER_ID)
                        .absolute()
                        .w(px(layout.width))
                        .h(px(placed.max_height))
                        .flex_row()
                        .occlude()
                        .child(Self::picker_rail(&picker.choosing, pal, cx))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .min_w_0()
                                .child(search_row(&picker.choosing.query, pal))
                                .child(Self::picker_rows(
                                    &picker.choosing,
                                    layout.descriptions,
                                    self.picker_empty_text(),
                                    pal,
                                    cx,
                                )),
                        ),
                ))
                .into_any_element(),
        )
    }
}
