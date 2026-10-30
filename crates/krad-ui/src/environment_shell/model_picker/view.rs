use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, FontWeight, InteractiveElement, IntoElement, KeyDownEvent,
    MouseButton, ParentElement, StatefulInteractiveElement, Styled, Window,
};
use krad_chat::ModelSource;
use krad_domain::Route;

use super::super::EnvironmentShell;
use super::{Choosing, HarnessRoute};
use crate::components::{harness_icon, icon, tooltip_view, IconName};
use crate::theme::{
    palette, popover_fill, popover_shadow, Palette, ICON, ICON_SM, LINE_BASE, LINE_XS, SPACE_1,
    SPACE_1_5, SPACE_2, SPACE_3, TEXT_BASE, TEXT_SM, TEXT_XS,
};

const PICKER_W: f32 = 340.0;
const PICKER_H: f32 = 380.0;
const RAIL_W: f32 = 48.0;
const RAIL_BUTTON: f32 = 34.0;
const SEARCH_H: f32 = 44.0;
const ROW_H: f32 = 32.0;
const PANEL_RADIUS: f32 = 14.0;
const BUTTON_RADIUS: f32 = 9.0;
const CARET_W: f32 = 1.5;
const CARET_H: f32 = 18.0;
const ANCHOR_GAP: f32 = 8.0;
const EDGE_MARGIN: f32 = 8.0;
const PICKER_ID: &str = "model-picker";
const LIST_ID: &str = "model-picker-list";
const RAIL_PREFIX: &str = "model-picker-harness";
const ROW_PREFIX: &str = "model-picker-row";
const SEARCH_PLACEHOLDER: &str = "Search models";
const NO_MODELS_TEXT: &str = "No models";
const SECTION_H: f32 = 26.0;
const GO_LABEL: &str = "OpenCode Go";
const ZEN_LABEL: &str = "OpenCode Zen";
const FREE_LABEL: &str = "Free";
const LANE_SEPARATOR: &str = " · ";
const GATEWAY_SETTINGS_ID: &str = "model-picker-gateway-settings";
const OPEN_SETTINGS_LABEL: &str = "Open Settings";

fn lane_name(lane: HarnessRoute) -> String {
    match lane.route {
        Route::Direct => lane.harness.display_name().to_owned(),
        Route::Gateway => format!(
            "{}{LANE_SEPARATOR}{}",
            lane.harness.display_name(),
            lane.route.label()
        ),
    }
}

fn lane_icon(lane: HarnessRoute, pal: &Palette) -> (IconName, u32) {
    match lane.route {
        Route::Direct => harness_icon(lane.harness, pal),
        Route::Gateway => (IconName::Flow, pal.accent),
    }
}

fn section_label(section: &ModelSource) -> &str {
    match section {
        ModelSource::OpencodeGo => GO_LABEL,
        ModelSource::OpencodeZen => ZEN_LABEL,
        ModelSource::Provider(name) => name,
        ModelSource::Free => FREE_LABEL,
    }
}

fn section_header(section: &ModelSource, pal: &Palette) -> AnyElement {
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
        .child(section_label(section).to_owned())
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
                let lane = group.lane;
                let selected = lane == choosing.lane;
                let (glyph, color) = lane_icon(lane, pal);
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
                    .tooltip(move |_, cx| tooltip_view(lane_name(lane).into(), cx))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.show_picker_lane(lane, cx);
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

    fn picker_rows(choosing: &Choosing, pal: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let shown = choosing.shown();
        let mut rows: Vec<AnyElement> = Vec::with_capacity(shown.len());
        let mut section = None;
        for (index, choice) in shown.iter().enumerate() {
            if let Some(current) = choice
                .section
                .as_ref()
                .filter(|_| section != Some(&choice.section))
            {
                rows.push(section_header(current, pal));
            }
            section = Some(&choice.section);
            let highlighted = index == choosing.highlighted;
            rows.push(
                div()
                    .id((ROW_PREFIX, element_index(index)))
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(SPACE_2))
                    .h(px(ROW_H))
                    .px(px(SPACE_2))
                    .rounded(px(BUTTON_RADIUS))
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
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(TEXT_BASE))
                            .line_height(px(LINE_BASE))
                            .text_color(rgb(pal.text_strong))
                            .child(choice.label.clone()),
                    )
                    .when(choice.current, |row| {
                        row.child(icon(IconName::Tick, ICON_SM, pal.text_strong))
                    })
                    .into_any_element(),
            );
        }
        let gateway_hint = choosing.lane.route == Route::Gateway && choosing.query.is_empty();
        let empty = shown.is_empty().then(|| {
            div()
                .flex()
                .flex_col()
                .gap(px(SPACE_2))
                .px(px(SPACE_2))
                .py(px(SPACE_2))
                .text_size(px(TEXT_SM))
                .text_color(rgb(pal.text_muted))
                .when(!gateway_hint, |empty| empty.child(NO_MODELS_TEXT))
                .when(gateway_hint, |empty| {
                    empty.child(Self::no_gateway_model_text()).child(
                        div()
                            .id(GATEWAY_SETTINGS_ID)
                            .cursor_pointer()
                            .text_color(rgb(pal.accent))
                            .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                                this.open_gateway_settings_from_picker(window, cx);
                            }))
                            .child(OPEN_SETTINGS_LABEL),
                    )
                })
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
        let height = PICKER_H.min(root_h - 2.0 * EDGE_MARGIN);
        let left = x.min(root_w - PICKER_W - EDGE_MARGIN).max(EDGE_MARGIN);
        let top = (y - height - ANCHOR_GAP)
            .min(root_h - height - EDGE_MARGIN)
            .max(EDGE_MARGIN);
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
                .on_key_down(cx.listener(|this: &mut Self, event: &KeyDownEvent, _, cx| {
                    this.on_model_picker_key(event, cx);
                }))
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .occlude()
                        .on_mouse_down(MouseButton::Left, cx.listener(dismiss))
                        .on_mouse_down(MouseButton::Right, cx.listener(dismiss)),
                )
                .child(
                    div()
                        .id(PICKER_ID)
                        .absolute()
                        .left(px(left))
                        .top(px(top))
                        .w(px(PICKER_W))
                        .h(px(height))
                        .flex()
                        .flex_row()
                        .rounded(px(PANEL_RADIUS))
                        .bg(popover_fill(pal, cx))
                        .border_1()
                        .border_color(pal.divider)
                        .shadow(popover_shadow(pal, cx))
                        .overflow_hidden()
                        .occlude()
                        .child(Self::picker_rail(&picker.choosing, pal, cx))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .min_w_0()
                                .child(search_row(&picker.choosing.query, pal))
                                .child(Self::picker_rows(&picker.choosing, pal, cx)),
                        ),
                )
                .into_any_element(),
        )
    }
}
