use gpui_kit::component::input::{EditorState, RopeExt as _};
use gpui_kit::{
    canvas, point, px, rgb, App, Bounds, ContentMask, Entity, IntoElement, Pixels, SharedString,
    Styled, TextAlign, TextRun, Window,
};

use crate::theme::current_theme;

const DIGIT_EM: f32 = 0.62;
const MIN_DIGITS: usize = 2;
const GUTTER_PAD: f32 = 10.0;
const DECIMAL: usize = 10;

fn digits(lines: usize) -> usize {
    let mut count = 1;
    let mut rest = lines / DECIMAL;
    while rest > 0 {
        count += 1;
        rest /= DECIMAL;
    }
    count.max(MIN_DIGITS)
}

#[allow(clippy::cast_precision_loss)]
pub(super) fn gutter_width(lines: usize, font_size: f32) -> f32 {
    digits(lines) as f32 * font_size * DIGIT_EM + 2.0 * GUTTER_PAD
}

fn paint_numbers(
    state: &Entity<EditorState>,
    area: Bounds<Pixels>,
    font_size: Pixels,
    window: &mut Window,
    cx: &mut App,
) {
    let editor = state.read(cx);
    let (Some(rows), Some(line_height)) = (editor.visible_row_range(), editor.line_height()) else {
        return;
    };
    let text = editor.text();
    let current = text.offset_to_position(editor.cursor()).line as usize;
    let theme = current_theme(cx);
    let (muted, strong) = (theme.syntax.comment.0, theme.terminal.foreground.0);
    let font = window.text_style().font();
    let width = area.size.width - px(GUTTER_PAD);
    let lines = text.lines_len();
    let placed: Vec<(usize, Pixels)> = rows
        .filter(|line| *line < lines)
        .filter_map(|line| {
            let start = text.line_start_offset(line);
            editor
                .range_to_bounds(&(start..start))
                .map(|bounds| (line, bounds.top()))
        })
        .collect();
    for (line, top) in placed {
        let label = SharedString::from((line + 1).to_string());
        let color = if line == current { strong } else { muted };
        let run = TextRun {
            len: label.len(),
            font: font.clone(),
            color: rgb(color).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let shaped = window
            .text_system()
            .shape_line(label, font_size, &[run], None);
        shaped
            .paint(
                point(area.left(), top),
                line_height,
                TextAlign::Right,
                Some(width),
                window,
                cx,
            )
            .unwrap_or_default();
    }
}

pub(super) fn line_numbers(
    state: Entity<EditorState>,
    width: f32,
    font_size: f32,
) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, (), window, cx| {
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                paint_numbers(&state, bounds, px(font_size), window, cx);
            });
        },
    )
    .flex_none()
    .h_full()
    .w(px(width))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gutter_widens_with_each_digit_of_the_last_line() {
        assert_eq!(digits(7), MIN_DIGITS);
        assert_eq!(digits(999), 3);
        assert_eq!(digits(12_000), 5);
        assert!(gutter_width(12_000, 14.0) > gutter_width(99, 14.0));
    }
}
