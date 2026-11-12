use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, point, px, rgb, size, AnyElement, Context, InteractiveElement, IntoElement, KeyDownEvent,
    MouseButton, ParentElement, SharedString, StatefulInteractiveElement, Styled, Window,
};
use krad_domain::{AgentPreset, ModelPickerLayout};

mod detail;

use super::marks::Mark;
use super::view::search_row;
use super::{Choosing, ModelChoice, Scope};
use crate::components::floating::{floating_at, floating_surface, place, Anchor, Side};
use crate::components::{harness_icon, icon, tooltip_view, IconName};
use crate::environment_shell::EnvironmentShell;
use crate::theme::{
    palette, Palette, ICON_SM, ICON_XS, LINE_SM, LINE_XS, SPACE_1, SPACE_1_5, SPACE_2, TEXT_SM,
    TEXT_XS,
};

const LIST_W: f32 = 300.0;
const LIST_H: f32 = 360.0;
const STACKED_LIST_H: f32 = 240.0;
const DETAIL_W: f32 = 312.0;
const DETAIL_H: f32 = 320.0;
const SHEET_GAP: f32 = 8.0;
const ANCHOR_GAP: f32 = 8.0;
const ROW_RADIUS: f32 = 10.0;
const CHIP_H: f32 = 26.0;
const CHIP_RADIUS: f32 = 13.0;
const LAYOUT_BUTTON: f32 = 24.0;
const DECK_MOTION_ID: &str = "model-deck-motion";
const DECK_ID: &str = "model-deck";
const ROWS_ID: &str = "model-deck-rows";
const ROW_PREFIX: &str = "model-deck-row";
const STAR_PREFIX: &str = "model-deck-star";
const SCOPE_PREFIX: &str = "model-deck-scope";
const LAYOUT_PREFIX: &str = "model-deck-layout";
const ALL_LABEL: &str = "All";
const FAVORITES_TOOLTIP: &str = "Favorites";
const DEFAULT_SUFFIX: &str = "default";
pub(super) const META_SEPARATOR: &str = " · ";
const SIDE_TOOLTIP: &str = "Details beside the list";
const STACKED_TOOLTIP: &str = "Details above the list";
pub(super) const TOOLTIP_FAVORITE: &str = "Favorite";

type ScopePick = Box<dyn Fn(&mut EnvironmentShell, &mut Context<EnvironmentShell>)>;

fn element_index(index: usize) -> u64 {
    u64::try_from(index).unwrap_or(u64::MAX)
}

pub(super) fn small_text(text: impl Into<SharedString>, color: u32) -> AnyElement {
    div()
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .text_color(rgb(color))
        .child(text.into())
        .into_any_element()
}

impl EnvironmentShell {
    pub(super) fn model_deck_overlay(
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
        let stacked = self.settings.beta.model_picker == ModelPickerLayout::Stacked;
        let (width, height) = if stacked {
            (DETAIL_W, DETAIL_H + STACKED_LIST_H)
        } else {
            (LIST_W + SHEET_GAP + DETAIL_W, LIST_H)
        };
        let placed = place(
            Anchor::Point(point(px(x), px(y - ANCHOR_GAP))),
            size(px(width), px(height)),
            size(px(root_w), px(root_h)),
            Side::Above,
        );
        let list = self.deck_list(&picker.choosing, stacked, pal, cx);
        let detail = self.deck_detail(&picker.choosing, picker.target, pal, cx);
        let body = if stacked {
            floating_surface(pal, cx)
                .id(DECK_ID)
                .absolute()
                .w(px(width))
                .flex_col()
                .occlude()
                .child(detail)
                .child(
                    div()
                        .h(px(STACKED_LIST_H))
                        .border_t_1()
                        .border_color(pal.divider)
                        .child(list),
                )
        } else {
            div()
                .id(DECK_ID)
                .absolute()
                .w(px(width))
                .h(px(height))
                .flex()
                .flex_row()
                .items_end()
                .gap(px(SHEET_GAP))
                .child(
                    floating_surface(pal, cx)
                        .occlude()
                        .w(px(LIST_W))
                        .h(px(LIST_H))
                        .child(list),
                )
                .child(
                    floating_surface(pal, cx)
                        .occlude()
                        .w(px(DETAIL_W))
                        .child(detail),
                )
        };
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
                .child(floating_at(DECK_MOTION_ID, placed, body))
                .into_any_element(),
        )
    }

    fn deck_list(
        &self,
        choosing: &Choosing,
        stacked: bool,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let search = div()
            .flex()
            .flex_row()
            .items_center()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(search_row(&choosing.query, pal)),
            )
            .child(Self::layout_toggle(stacked, pal, cx));
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(search)
            .child(Self::scope_chips(choosing, pal, cx))
            .child(self.deck_rows(choosing, pal, cx))
            .into_any_element()
    }

    fn layout_toggle(stacked: bool, pal: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let button = |layout: ModelPickerLayout,
                      glyph: IconName,
                      tooltip: &'static str,
                      cx: &mut Context<Self>| {
            let on = (layout == ModelPickerLayout::Stacked) == stacked;
            div()
                .id((LAYOUT_PREFIX, layout as u64))
                .flex()
                .items_center()
                .justify_center()
                .size(px(LAYOUT_BUTTON))
                .rounded(px(SPACE_1_5))
                .cursor_pointer()
                .when(on, |button| button.bg(pal.bg_selected))
                .when(!on, |button| button.hover(|style| style.bg(pal.bg_hover)))
                .tooltip(move |_, cx| tooltip_view(tooltip.into(), cx))
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.settings.beta.model_picker = layout;
                    this.save_settings(cx);
                    cx.notify();
                }))
                .child(icon(
                    glyph,
                    ICON_XS,
                    if on { pal.text_strong } else { pal.text_muted },
                ))
        };
        div()
            .flex()
            .flex_row()
            .flex_none()
            .gap(px(SPACE_1 / 2.0))
            .pr(px(SPACE_2))
            .child(button(
                ModelPickerLayout::Side,
                IconName::ColumnsTwo,
                SIDE_TOOLTIP,
                cx,
            ))
            .child(button(
                ModelPickerLayout::Stacked,
                IconName::RowsTwo,
                STACKED_TOOLTIP,
                cx,
            ))
            .into_any_element()
    }

    fn scope_chips(choosing: &Choosing, pal: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let chip = |key: u64,
                    on: bool,
                    tooltip: SharedString,
                    content: AnyElement,
                    pick: ScopePick,
                    cx: &mut Context<Self>| {
            div()
                .id((SCOPE_PREFIX, key))
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .h(px(CHIP_H))
                .min_w(px(CHIP_H))
                .px(px(SPACE_2))
                .rounded(px(CHIP_RADIUS))
                .cursor_pointer()
                .when(on, |chip| chip.bg(pal.bg_selected))
                .when(!on, |chip| chip.hover(|style| style.bg(pal.bg_hover)))
                .tooltip(move |_, cx| tooltip_view(tooltip.clone(), cx))
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| pick(this, cx)))
                .child(content)
                .into_any_element()
        };
        let all = chip(
            0,
            choosing.scope == Scope::All,
            ALL_LABEL.into(),
            small_text(
                ALL_LABEL,
                if choosing.scope == Scope::All {
                    pal.text_strong
                } else {
                    pal.text_muted
                },
            ),
            Box::new(|this, cx| this.show_picker_scope(Scope::All, cx)),
            cx,
        );
        let favorites = chip(
            1,
            choosing.scope == Scope::Favorites,
            FAVORITES_TOOLTIP.into(),
            icon(IconName::Star, ICON_XS, pal.text_muted).into_any_element(),
            Box::new(|this, cx| this.show_picker_scope(Scope::Favorites, cx)),
            cx,
        );
        let harnesses: Vec<AnyElement> = choosing
            .groups
            .iter()
            .enumerate()
            .map(|(index, group)| {
                let harness = group.harness;
                let (glyph, color) = harness_icon(harness, pal);
                chip(
                    element_index(index) + 2,
                    choosing.scope == Scope::Harness && choosing.harness == harness,
                    group.name.clone(),
                    icon(glyph, ICON_SM, color).into_any_element(),
                    Box::new(move |this, cx| this.show_picker_harness(harness, cx)),
                    cx,
                )
            })
            .collect();
        div()
            .flex()
            .flex_row()
            .flex_none()
            .gap(px(SPACE_1 / 2.0))
            .px(px(SPACE_2))
            .py(px(SPACE_1_5))
            .border_b_1()
            .border_color(pal.divider)
            .child(all)
            .child(favorites)
            .children(harnesses)
            .into_any_element()
    }

    fn deck_rows(&self, choosing: &Choosing, pal: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let entries = choosing.entries();
        let rows: Vec<AnyElement> = entries
            .iter()
            .enumerate()
            .map(|(index, (harness, choice))| {
                Self::deck_row(index, *harness, choice, choosing, pal, cx)
            })
            .collect();
        let empty = entries.is_empty().then(|| {
            div()
                .p(px(SPACE_2))
                .text_size(px(TEXT_SM))
                .text_color(rgb(pal.text_muted))
                .child(self.picker_empty_text())
        });
        div()
            .id(ROWS_ID)
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

    fn deck_row(
        index: usize,
        harness: AgentPreset,
        choice: &ModelChoice,
        choosing: &Choosing,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let highlighted = index == choosing.highlighted;
        let (glyph, color) = harness_icon(harness, pal);
        let group_name = choosing
            .groups
            .iter()
            .find(|group| group.harness == harness)
            .map_or_else(|| harness.display_name().into(), |group| group.name.clone());
        let sub = if choice.marks.default {
            format!("{group_name}{META_SEPARATOR}{DEFAULT_SUFFIX}")
        } else {
            group_name.to_string()
        };
        let star = (choice.id.is_some() && (highlighted || choice.marks.favorite)).then(|| {
            div()
                .id((STAR_PREFIX, element_index(index)))
                .flex_none()
                .cursor_pointer()
                .tooltip(|_, cx| tooltip_view(TOOLTIP_FAVORITE.into(), cx))
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    cx.stop_propagation();
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
        });
        div()
            .id((ROW_PREFIX, element_index(index)))
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SPACE_2))
            .px(px(SPACE_2))
            .py(px(SPACE_1_5))
            .rounded(px(ROW_RADIUS))
            .cursor_pointer()
            .when(highlighted, |row| row.bg(pal.bg_hover))
            .on_hover(cx.listener(move |this: &mut Self, hovered: &bool, _, cx| {
                if *hovered {
                    this.highlight_model(index, cx);
                }
            }))
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.pick_model(index, cx);
            }))
            .child(icon(glyph, ICON_SM, color))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(TEXT_SM))
                            .line_height(px(LINE_SM))
                            .text_color(rgb(pal.text_strong))
                            .child(choice.label.clone()),
                    )
                    .child(small_text(sub, pal.text_muted)),
            )
            .children(star)
            .child(
                div()
                    .flex_none()
                    .w(px(ICON_SM))
                    .when(choice.current, |slot| {
                        slot.child(icon(IconName::Tick, ICON_SM, pal.accent))
                    }),
            )
            .into_any_element()
    }
}
