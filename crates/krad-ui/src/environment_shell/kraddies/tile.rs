use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, App, Div, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, SharedString, Stateful, StatefulInteractiveElement, Styled,
};

use super::layout::CARD_PAD;
use super::motion::entering;
use super::rows::{control_room, row_shape, RowShape, BUTTON_PAD, LABEL_MIN_W, ROW_GAP};
use crate::components::label_width;
use crate::theme::{
    Palette, LINE_SM, LINE_XS, RADIUS_XL, SPACE_0_5, SPACE_1, SPACE_2, SPACE_3, TEXT_SM, TEXT_XS,
};

const WRAP_SLACK: f32 = 0.5;
const TILE_BORDER: f32 = 1.0;

pub(super) struct Tile {
    pub(super) id: SharedString,
    pub(super) index: usize,
    pub(super) width: f32,
    pub(super) title: &'static str,
    pub(super) note: Option<SharedString>,
    pub(super) accessory: Option<AnyElement>,
    pub(super) body: AnyElement,
}

pub(super) fn tile(tile: Tile, pal: &'static Palette) -> AnyElement {
    let Tile {
        id,
        index,
        width,
        title,
        note,
        accessory,
        body,
    } = tile;
    let header = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .justify_between()
        .gap(px(SPACE_2))
        .child(
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .gap(px(SPACE_0_5))
                .child(
                    div()
                        .text_size(px(TEXT_SM))
                        .line_height(px(LINE_SM))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(pal.text_strong))
                        .child(title),
                )
                .children(note.map(|note| caption(note, pal))),
        )
        .children(accessory);
    let frame = div()
        .id(ElementId::from(id.clone()))
        .flex()
        .flex_col()
        .flex_none()
        .w(px((width - WRAP_SLACK).max(0.0)))
        .min_w_0()
        .gap(px(SPACE_3))
        .p(px(CARD_PAD))
        .rounded(px(RADIUS_XL))
        .child(header)
        .child(body);
    entering(lifted(frame, pal), (id, index), index).into_any_element()
}

#[must_use]
pub(super) fn tile_inner(width: f32) -> f32 {
    (width - WRAP_SLACK - 2.0 * (CARD_PAD + TILE_BORDER)).max(0.0)
}

pub(super) fn caption(text: impl Into<SharedString>, pal: &Palette) -> Div {
    div()
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .text_color(rgb(pal.text_muted))
        .child(text.into())
}

pub(super) fn grid(gutter: f32, tiles: Vec<AnyElement>) -> Div {
    div()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_start()
        .gap(px(gutter))
        .children(tiles)
}

pub(super) fn lifted(frame: Stateful<Div>, pal: &Palette) -> Stateful<Div> {
    let (rest, hover, edge, pressed) = (pal.bg_panel, pal.bg_row_selected, pal.border, pal.divider);
    frame
        .bg(rgb(rest))
        .border_1()
        .border_color(pal.divider)
        .hover(move |style| style.bg(rgb(hover)).border_color(edge))
        .active(move |style| style.bg(rgb(rest)).border_color(pressed))
}

#[must_use]
pub(super) fn button_w(label: &str, cx: &App) -> f32 {
    label_width(label, TEXT_SM, FontWeight::MEDIUM, cx) + BUTTON_PAD
}

pub(super) struct SettingRow {
    pub(super) label: SharedString,
    pub(super) detail: Option<SharedString>,
    pub(super) control_w: f32,
    pub(super) card_inner: f32,
}

impl SettingRow {
    pub(super) fn new(label: impl Into<SharedString>, control_w: f32, card_inner: f32) -> Self {
        Self {
            label: label.into(),
            detail: None,
            control_w,
            card_inner,
        }
    }

    pub(super) fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    fn label_w(&self, cx: &App) -> f32 {
        label_width(&self.label, TEXT_SM, FontWeight::NORMAL, cx)
    }

    fn shape(&self, cx: &App) -> RowShape {
        row_shape(self.card_inner, self.label_w(cx), self.control_w)
    }

    #[must_use]
    pub(super) fn room(&self, cx: &App) -> f32 {
        control_room(self.shape(cx), self.card_inner, self.label_w(cx))
    }
}

pub(super) fn setting_row(
    row: SettingRow,
    control: Option<AnyElement>,
    pal: &Palette,
    cx: &App,
) -> AnyElement {
    let label_w = row.label_w(cx);
    let shape = row.shape(cx);
    let fits = label_w <= row.card_inner;
    let text = div()
        .flex()
        .flex_col()
        .min_w(px(label_w.max(LABEL_MIN_W).min(row.card_inner)))
        .max_w(px(row.card_inner))
        .gap(px(SPACE_0_5))
        .child(
            div()
                .whitespace_nowrap()
                .when(!fits, Styled::truncate)
                .text_size(px(TEXT_SM))
                .line_height(px(LINE_SM))
                .text_color(rgb(pal.text_default))
                .child(row.label),
        )
        .children(row.detail.map(|detail| caption(detail, pal)));
    let line = div().flex().gap(px(ROW_GAP)).py(px(SPACE_1));
    let line = match shape {
        RowShape::Beside => line
            .flex_row()
            .items_center()
            .justify_between()
            .child(text.flex_1().flex_shrink_0()),
        RowShape::Stacked => line.flex_col().items_start().child(text),
    };
    line.children(control.map(|control| div().flex_none().child(control)))
        .into_any_element()
}
