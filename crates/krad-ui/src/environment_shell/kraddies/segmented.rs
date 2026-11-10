use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnimationExt as _, AnyElement, App, Div, FontWeight, InteractiveElement,
    IntoElement, ParentElement, SharedString, SpringAnimation, Stateful,
    StatefulInteractiveElement, Styled,
};

use super::motion::{entering, SLIDE_SPRING};
use crate::components::{
    fit, icon, label_width, track_width, IconName, SegmentFit, Span, Stretch, SEGMENT_GAP,
    TRACK_PAD,
};
use crate::theme::{
    row_fill, row_shadow, Palette, ICON_SM, LINE_XS, RADIUS_LG, SPACE_1, SPACE_2, TAB_H, TEXT_XS,
};

const PILL_SUFFIX: &str = "pill";
const WIDTH_SUFFIX: &str = "width";
const MENU_SUFFIX: &str = "menu";
const OPTION_SUFFIX: &str = "option";
const PRESSED_OPACITY: f32 = 0.72;

pub(super) struct Sliding<'a> {
    pub(super) id: &'static str,
    pub(super) labels: &'a [&'static str],
    pub(super) short: &'a [&'static str],
    pub(super) chosen: usize,
    pub(super) container: f32,
    pub(super) stretch: Stretch,
    pub(super) menu_open: bool,
}

#[must_use]
pub(super) fn sliding_width(labels: &[&'static str], cx: &App) -> f32 {
    track_width(&measure(labels, cx))
}

fn measure(labels: &[&'static str], cx: &App) -> Vec<f32> {
    labels
        .iter()
        .map(|label| label_width(label, TEXT_XS, FontWeight::MEDIUM, cx))
        .collect()
}

fn segment_text(label: &'static str, picked: bool, pal: &Palette) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .h(px(TAB_H))
        .rounded_full()
        .cursor_pointer()
        .whitespace_nowrap()
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .text_color(rgb(if picked {
            pal.text_strong
        } else {
            pal.text_muted
        }))
        .when(picked, |segment| segment.font_weight(FontWeight::MEDIUM))
        .child(label)
}

pub(super) fn sliding_track(
    sliding: &Sliding<'_>,
    pal: &Palette,
    cx: &App,
    on_pick: impl FnMut(usize, Stateful<Div>) -> Stateful<Div>,
    on_menu: impl FnOnce(Stateful<Div>) -> Stateful<Div>,
) -> AnyElement {
    let full = measure(sliding.labels, cx);
    let short = measure(sliding.short, cx);
    match fit(&full, &short, sliding.container, sliding.stretch) {
        SegmentFit::Full(spans) => track(sliding, sliding.labels, &spans, pal, cx, on_pick),
        SegmentFit::Short(spans) => track(sliding, sliding.short, &spans, pal, cx, on_pick),
        SegmentFit::Menu { width } => menu(sliding, width, pal, on_pick, on_menu),
    }
}

fn track(
    sliding: &Sliding<'_>,
    labels: &[&'static str],
    spans: &[Span],
    pal: &Palette,
    cx: &App,
    mut on_pick: impl FnMut(usize, Stateful<Div>) -> Stateful<Div>,
) -> AnyElement {
    let id = sliding.id;
    let chosen = spans.get(sliding.chosen).copied();
    let inner_w = spans.last().map_or(0.0, |span| span.right());
    let pill = chosen.map(|span| {
        let shape = div()
            .h(px(TAB_H))
            .rounded_full()
            .bg(row_fill(pal, cx))
            .border_1()
            .border_color(pal.divider)
            .shadow(row_shadow(pal, cx))
            .with_spring(
                (SharedString::from(format!("{id}-{WIDTH_SUFFIX}")), 0_usize),
                SpringAnimation::new(SLIDE_SPRING).to(px(span.width)),
                Styled::w,
            );
        div()
            .absolute()
            .top(px(TRACK_PAD))
            .child(shape)
            .with_spring(
                (SharedString::from(format!("{id}-{PILL_SUFFIX}")), 0_usize),
                SpringAnimation::new(SLIDE_SPRING).to(px(TRACK_PAD + span.left)),
                Styled::left,
            )
    });
    let hover = pal.text_default;
    let segments = labels
        .iter()
        .zip(spans)
        .enumerate()
        .map(|(index, (label, span))| {
            let picked = index == sliding.chosen;
            let segment = segment_text(label, picked, pal)
                .id((id, index))
                .w(px(span.width))
                .when(!picked, |segment| {
                    segment.hover(move |style| style.text_color(rgb(hover)))
                })
                .active(|style| style.opacity(PRESSED_OPACITY));
            on_pick(index, segment)
        });
    div()
        .relative()
        .flex()
        .flex_row()
        .flex_none()
        .gap(px(SEGMENT_GAP))
        .w(px(inner_w + 2.0 * TRACK_PAD))
        .p(px(TRACK_PAD))
        .rounded_full()
        .bg(pal.bg_hover)
        .children(pill)
        .children(segments)
        .into_any_element()
}

fn menu(
    sliding: &Sliding<'_>,
    width: f32,
    pal: &Palette,
    mut on_pick: impl FnMut(usize, Stateful<Div>) -> Stateful<Div>,
    on_menu: impl FnOnce(Stateful<Div>) -> Stateful<Div>,
) -> AnyElement {
    let id = sliding.id;
    let current = sliding
        .labels
        .get(sliding.chosen)
        .copied()
        .unwrap_or_default();
    let glyph = if sliding.menu_open {
        IconName::ChevronUp
    } else {
        IconName::ChevronDown
    };
    let button = div()
        .id(SharedString::from(format!("{id}-{MENU_SUFFIX}")))
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(SPACE_1))
        .w(px(width))
        .h(px(TAB_H + 2.0 * TRACK_PAD))
        .px(px(SPACE_2))
        .rounded_full()
        .cursor_pointer()
        .bg(pal.bg_hover)
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(pal.text_strong))
        .active(|style| style.opacity(PRESSED_OPACITY))
        .child(div().min_w_0().child(current))
        .child(icon(glyph, ICON_SM, pal.text_muted));
    let hover = pal.bg_hover;
    let options = sliding.menu_open.then(|| {
        let rows = sliding.labels.iter().enumerate().map(|(index, label)| {
            let picked = index == sliding.chosen;
            let row = segment_text(label, picked, pal)
                .id((SharedString::from(format!("{id}-{OPTION_SUFFIX}")), index))
                .w_full()
                .justify_start()
                .whitespace_normal()
                .px(px(SPACE_2))
                .rounded(px(RADIUS_LG))
                .hover(move |style| style.bg(hover));
            entering(on_pick(index, row), (id, index), index).into_any_element()
        });
        div()
            .flex()
            .flex_col()
            .w(px(width))
            .p(px(TRACK_PAD))
            .rounded(px(RADIUS_LG))
            .bg(rgb(pal.bg_surface))
            .border_1()
            .border_color(pal.divider)
            .children(rows)
    });
    div()
        .flex()
        .flex_col()
        .gap(px(SPACE_1))
        .child(on_menu(button))
        .children(options)
        .into_any_element()
}
