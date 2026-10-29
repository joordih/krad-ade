use gpui_kit::{
    div, px, rgb, AnyElement, Context, Div, ElementId, FontWeight, InteractiveElement, IntoElement,
    MouseButton, ParentElement, SharedString, Stateful, StatefulInteractiveElement, Styled, Window,
};

use super::super::EnvironmentShell;
use crate::components::{Button, ButtonVariant, IconName};
use crate::theme::{
    floating_shadow, palette, scrim_fill, Palette, LINE_LG, LINE_SM, LINE_XS, RADIUS_LG, SPACE_0_5,
    SPACE_2, SPACE_3, SPACE_6, TEXT_LG, TEXT_SM, TEXT_XS,
};

const SHEET_MARGIN: f32 = 32.0;
const SHEET_RADIUS: f32 = 16.0;
const TITLE_H: f32 = 60.0;
const SECTION_GAP: f32 = 28.0;
const SECTION_HEADER_GAP: f32 = 10.0;
const ROW_MIN_H: f32 = 56.0;
const ROW_PAD_X: f32 = 16.0;
const ROW_PAD_Y: f32 = 10.0;
const ROW_GAP: f32 = 16.0;
const RULE: f32 = 1.0;
const FOOTER_PAD_Y: f32 = 14.0;

pub(in crate::environment_shell) type Dismiss =
    fn(&mut EnvironmentShell, &mut Window, &mut Context<EnvironmentShell>);

pub(in crate::environment_shell) fn sheet_overlay(
    sheet: impl IntoElement,
    dismiss: Dismiss,
    cx: &mut Context<EnvironmentShell>,
) -> AnyElement {
    let pal = palette(cx);
    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .p(px(SHEET_MARGIN))
        .bg(scrim_fill(pal, cx))
        .occlude()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this: &mut EnvironmentShell, _, window, cx| {
                dismiss(this, window, cx);
            }),
        )
        .child(sheet)
        .into_any_element()
}

pub(in crate::environment_shell) fn sheet_panel(width: f32, pal: &Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .w_full()
        .max_w(px(width))
        .max_h_full()
        .rounded(px(SHEET_RADIUS))
        .bg(rgb(pal.bg_surface))
        .border_1()
        .border_color(pal.border)
        .shadow(floating_shadow(pal))
        .overflow_hidden()
        .occlude()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
}

pub(in crate::environment_shell) fn sheet_title(
    title: impl Into<SharedString>,
    subtitle: Option<String>,
    close_id: &'static str,
    dismiss: Dismiss,
    cx: &mut Context<EnvironmentShell>,
) -> AnyElement {
    let pal = palette(cx);
    div()
        .flex()
        .flex_row()
        .flex_none()
        .items_center()
        .gap(px(SPACE_3))
        .h(px(TITLE_H))
        .pl(px(SPACE_6))
        .pr(px(SPACE_3))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(TEXT_LG))
                        .line_height(px(LINE_LG))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(pal.text_strong))
                        .child(title.into()),
                )
                .children(subtitle.map(|subtitle| {
                    div()
                        .truncate()
                        .text_size(px(TEXT_XS))
                        .line_height(px(LINE_XS))
                        .text_color(rgb(pal.text_muted))
                        .child(subtitle)
                })),
        )
        .child(
            Button::new(close_id)
                .icon(IconName::Close)
                .variant(ButtonVariant::Ghost)
                .on_click(
                    cx.listener(move |this: &mut EnvironmentShell, _, window, cx| {
                        dismiss(this, window, cx);
                    }),
                ),
        )
        .into_any_element()
}

pub(in crate::environment_shell) fn sheet_body(id: impl Into<ElementId>) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .gap(px(SECTION_GAP))
        .px(px(SPACE_6))
        .pb(px(SPACE_6))
}

pub(in crate::environment_shell) fn sheet_section(
    label: impl Into<SharedString>,
    description: Option<&str>,
    pal: &Palette,
) -> Div {
    div().flex().flex_col().gap(px(SECTION_HEADER_GAP)).child(
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_0_5))
            .child(
                div()
                    .text_size(px(TEXT_SM))
                    .line_height(px(LINE_SM))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(pal.text_strong))
                    .child(label.into()),
            )
            .children(description.map(|text| {
                div()
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .text_color(rgb(pal.text_muted))
                    .child(text.to_owned())
            })),
    )
}

pub(in crate::environment_shell) fn sheet_group(
    rows: impl IntoIterator<Item = AnyElement>,
    pal: &Palette,
) -> AnyElement {
    let mut children = Vec::new();
    for row in rows {
        if !children.is_empty() {
            children.push(
                div()
                    .h(px(RULE))
                    .mx(px(ROW_PAD_X))
                    .bg(pal.divider)
                    .into_any_element(),
            );
        }
        children.push(row);
    }
    div()
        .flex()
        .flex_col()
        .rounded(px(RADIUS_LG))
        .bg(rgb(pal.bg_panel))
        .border_1()
        .border_color(pal.divider)
        .overflow_hidden()
        .children(children)
        .into_any_element()
}

pub(in crate::environment_shell) fn sheet_row(
    title: impl Into<SharedString>,
    detail: Option<SharedString>,
    control: Option<AnyElement>,
    pal: &Palette,
) -> Div {
    sheet_row_with(None, title, detail, control, pal)
}

pub(in crate::environment_shell) fn sheet_row_with(
    leading: Option<AnyElement>,
    title: impl Into<SharedString>,
    detail: Option<SharedString>,
    control: Option<AnyElement>,
    pal: &Palette,
) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(ROW_GAP))
        .min_h(px(ROW_MIN_H))
        .px(px(ROW_PAD_X))
        .py(px(ROW_PAD_Y))
        .children(leading.map(|leading| div().flex_none().child(leading)))
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
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(pal.text_strong))
                        .child(title.into()),
                )
                .children(detail.map(|detail| {
                    div()
                        .text_size(px(TEXT_XS))
                        .line_height(px(LINE_XS))
                        .text_color(rgb(pal.text_muted))
                        .child(detail)
                })),
        )
        .children(control.map(|control| div().flex_none().child(control)))
}

pub(in crate::environment_shell) fn sheet_footer(pal: &Palette) -> Div {
    div()
        .flex()
        .flex_row()
        .flex_none()
        .items_center()
        .gap(px(SPACE_2))
        .px(px(SPACE_6))
        .py(px(FOOTER_PAD_Y))
        .border_t_1()
        .border_color(pal.divider)
}
