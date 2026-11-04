use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Div, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, SharedString, Stateful, Styled,
};

use super::super::dialog_parts::sheet_padded;
use super::gitlab::page::{field_row, section_heading, status_detail, status_mark};
use super::{hint, SWATCH_GAP};
use crate::components::{icon, IconName};
use crate::theme::{
    Palette, ICON_SM, LINE_SM, SPACE_0_5, SPACE_1_5, SPACE_2, SPACE_3, SPACE_6, TEXT_SM,
};

const PANE_GAP: f32 = SPACE_6;
const HEADING_GAP: f32 = SPACE_2;

pub(super) fn pane_column(sections: impl IntoIterator<Item = AnyElement>) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(PANE_GAP))
        .children(sections)
        .into_any_element()
}

pub(super) fn settings_section(
    title: &str,
    detail: Option<&str>,
    body: impl IntoIterator<Item = AnyElement>,
    pal: &Palette,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(HEADING_GAP))
        .child(section_heading(title, detail, pal))
        .child(div().flex().flex_col().gap(px(SWATCH_GAP)).children(body))
        .into_any_element()
}

pub(super) fn empty_row(text: &str, pal: &Palette) -> AnyElement {
    sheet_padded(
        div()
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .text_color(rgb(pal.text_muted))
            .child(SharedString::from(text.to_owned())),
    )
    .into_any_element()
}

pub(super) fn card_field(label: &str, field: AnyElement, pal: &Palette) -> AnyElement {
    sheet_padded(field_row(label, field, pal)).into_any_element()
}

pub(super) fn footnote(text: &str, pal: &Palette) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .gap(px(SPACE_2))
        .child(icon(IconName::Lock, ICON_SM, pal.text_muted))
        .child(div().flex_1().min_w_0().child(hint(text, pal)))
        .into_any_element()
}

pub(super) fn disclosure(
    id: impl Into<ElementId>,
    label: &str,
    open: bool,
    pal: &Palette,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_row()
        .items_center()
        .gap(px(SPACE_1_5))
        .cursor_pointer()
        .text_size(px(TEXT_SM))
        .line_height(px(LINE_SM))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(pal.text_default))
        .child(icon(
            if open {
                IconName::ChevronDown
            } else {
                IconName::ChevronRight
            },
            ICON_SM,
            pal.text_muted,
        ))
        .child(SharedString::from(label.to_owned()))
}

pub(super) struct AccountRow {
    pub(super) mark: IconName,
    pub(super) name: &'static str,
    pub(super) state: String,
    pub(super) detail: String,
    pub(super) connected: bool,
    pub(super) actions: Vec<AnyElement>,
    pub(super) below: Option<AnyElement>,
}

pub(super) fn account_row(row: AccountRow, pal: &Palette, cx: &gpui_kit::App) -> AnyElement {
    let AccountRow {
        mark,
        name,
        state,
        detail,
        connected,
        actions,
        below,
    } = row;
    let line = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(SPACE_3))
        .child(status_mark(mark, connected, pal, cx))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap(px(SPACE_0_5))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_baseline()
                        .gap(px(SPACE_2))
                        .text_size(px(TEXT_SM))
                        .line_height(px(LINE_SM))
                        .child(
                            div()
                                .flex_none()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(pal.text_strong))
                                .child(name),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_color(rgb(if connected {
                                    pal.success_fg
                                } else {
                                    pal.text_muted
                                }))
                                .child(state),
                        ),
                )
                .when(!detail.is_empty(), |text| {
                    text.child(status_detail(&detail, connected, pal))
                }),
        )
        .child(
            div()
                .flex()
                .flex_none()
                .flex_row()
                .gap(px(SPACE_2))
                .children(actions),
        );
    sheet_padded(
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_3))
            .child(line)
            .children(below),
    )
    .into_any_element()
}
