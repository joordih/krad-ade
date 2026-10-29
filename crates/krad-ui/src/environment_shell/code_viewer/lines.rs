use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::component::ActiveTheme;
use gpui_kit::{
    combine_highlights, div, px, rgb, uniform_list, AnyElement, ClipboardItem, Context,
    HighlightStyle, InteractiveElement, IntoElement, ListHorizontalSizingBehavior, MouseButton,
    MouseDownEvent, MouseMoveEvent, ParentElement, Pixels, Point, Styled, StyledText, TextLayout,
    Window,
};
use std::ops::Range;
use std::sync::Arc;

use super::super::drag::DragState;
use super::super::EnvironmentShell;
use super::selection::{CodeSelection, TextPoint};
use super::{CodeView, ViewerContent, ViewerText};
use crate::actions::{CopySelection, VIEWER_KEY_CONTEXT};
use crate::highlight::{Span, TokenKind};
use crate::theme::{palette, tinted, Palette, SPACE_1, SPACE_3, TEXT_SM};

const VIEWER_LIST_ID: &str = "code-viewer-lines";
const CODE_FONT_PX: f32 = TEXT_SM;
const CODE_LINE_PX: f32 = 20.0;
const SELECTION_ALPHA: u32 = 0x40;
const GUTTER_DIGIT_PX: f32 = 8.0;
const GUTTER_PAD_PX: f32 = 16.0;

const fn token_color(kind: TokenKind, pal: &Palette) -> u32 {
    match kind {
        TokenKind::Keyword => pal.syntax.keyword,
        TokenKind::String => pal.syntax.string,
        TokenKind::Comment => pal.syntax.comment,
        TokenKind::Number => pal.syntax.number,
        TokenKind::TypeName => pal.syntax.type_name,
    }
}

#[allow(clippy::cast_precision_loss)]
fn gutter_width(line_count: usize) -> f32 {
    let digits = line_count.max(1).ilog10() + 1;
    digits as f32 * GUTTER_DIGIT_PX + GUTTER_PAD_PX
}

impl EnvironmentShell {
    fn start_code_selection(&mut self, point: TextPoint, cx: &mut Context<Self>) {
        if let Some(viewer) = self.viewer.as_mut() {
            viewer.selection = Some(CodeSelection::at(point));
            self.drag = DragState::SelectCode;
            cx.notify();
        }
    }

    fn extend_code_selection(&mut self, head: TextPoint, cx: &mut Context<Self>) {
        if !matches!(self.drag, DragState::SelectCode) {
            return;
        }
        let Some(viewer) = self.viewer.as_mut() else {
            return;
        };
        let extended = viewer
            .selection
            .map(|selection| selection.extended_to(head));
        if extended != viewer.selection {
            viewer.selection = extended;
            cx.notify();
        }
    }

    fn on_copy_code(&mut self, _: &CopySelection, _: &mut Window, cx: &mut Context<Self>) {
        let Some(viewer) = self.viewer.as_ref() else {
            return;
        };
        let (Some(selection), ViewerContent::Text(text)) = (viewer.selection, &viewer.content)
        else {
            return;
        };
        let copied = selection.text(&text.lines);
        if !copied.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(copied));
        }
    }

    pub(super) fn code_lines(
        text: &Arc<ViewerText>,
        viewer: &CodeView,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mono = cx.theme().mono_font_family.clone();
        let gutter = gutter_width(text.lines.len());
        let widest = text.widest;
        let selection = viewer.selection;
        let lines_text = Arc::clone(text);
        let list = uniform_list(
            VIEWER_LIST_ID,
            text.lines.len(),
            cx.processor(
                move |_: &mut Self, range: Range<usize>, _: &mut Window, cx| {
                    let pal = palette(cx);
                    range
                        .map(|index| {
                            let line = lines_text.lines[index].clone();
                            let selected = selection
                                .and_then(|selection| selection.range_on(index, line.len()))
                                .map(|range| (range, selection_style(pal)));
                            let highlights: Vec<(Range<usize>, HighlightStyle)> =
                                combine_highlights(
                                    syntax_highlights(&lines_text.spans[index], pal),
                                    selected,
                                )
                                .collect();
                            let styled = StyledText::new(line).with_highlights(highlights);
                            let press_layout = styled.layout().clone();
                            let drag_layout = press_layout.clone();
                            div()
                                .flex()
                                .flex_row()
                                .h(px(CODE_LINE_PX))
                                .font_family(mono.clone())
                                .text_size(px(CODE_FONT_PX))
                                .line_height(px(CODE_LINE_PX))
                                .whitespace_nowrap()
                                .cursor_text()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(
                                        move |this: &mut Self, event: &MouseDownEvent, _, cx| {
                                            let point =
                                                text_point(index, &press_layout, event.position);
                                            this.start_code_selection(point, cx);
                                        },
                                    ),
                                )
                                .on_mouse_move(cx.listener(
                                    move |this: &mut Self, event: &MouseMoveEvent, _, cx| {
                                        let point = text_point(index, &drag_layout, event.position);
                                        this.extend_code_selection(point, cx);
                                    },
                                ))
                                .child(
                                    div()
                                        .flex_none()
                                        .w(px(gutter))
                                        .pr(px(SPACE_3))
                                        .flex()
                                        .justify_end()
                                        .text_color(rgb(pal.text_muted))
                                        .child((index + 1).to_string()),
                                )
                                .child(
                                    div()
                                        .flex_none()
                                        .pr(px(SPACE_3))
                                        .text_color(rgb(pal.text_strong))
                                        .child(styled),
                                )
                        })
                        .collect::<Vec<_>>()
                },
            ),
        )
        .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
        .with_width_from_item(Some(widest))
        .track_scroll(&viewer.scroll)
        .size_full()
        .pt(px(SPACE_1));
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .track_focus(&viewer.focus)
            .key_context(VIEWER_KEY_CONTEXT)
            .on_action(cx.listener(Self::on_copy_code))
            .child(list)
            .child(Scrollbar::new(&viewer.scroll))
            .into_any_element()
    }
}

fn syntax_highlights(spans: &[Span], pal: &Palette) -> Vec<(Range<usize>, HighlightStyle)> {
    spans
        .iter()
        .map(|span| {
            (
                span.range.clone(),
                HighlightStyle {
                    color: Some(rgb(token_color(span.kind, pal)).into()),
                    ..HighlightStyle::default()
                },
            )
        })
        .collect()
}

fn selection_style(pal: &Palette) -> HighlightStyle {
    HighlightStyle {
        background_color: Some(tinted(pal.accent, SELECTION_ALPHA).into()),
        ..HighlightStyle::default()
    }
}

fn text_point(line: usize, layout: &TextLayout, position: Point<Pixels>) -> TextPoint {
    let byte = layout
        .index_for_position(position)
        .unwrap_or_else(|nearest| nearest);
    TextPoint { line, byte }
}
