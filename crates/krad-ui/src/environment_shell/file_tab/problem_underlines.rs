use gpui_kit::component::input::{EditorState, RopeExt as _};
use gpui_kit::{
    canvas, point, px, rgb, AnyElement, App, Bounds, ContentMask, Entity, IntoElement, Pixels,
    Styled, UnderlineStyle, Window,
};
use krad_domain::ProblemSeverity;
use std::ops::Range;

use super::problem_list::ProblemList;
use crate::theme::{palette, Palette};

const UNDERLINE_THICKNESS_PX: f32 = 1.0;
const MIN_UNDERLINE_PX: f32 = 6.0;

const fn underline_color(severity: ProblemSeverity, pal: &Palette) -> u32 {
    match severity {
        ProblemSeverity::Error => pal.error_fg,
        ProblemSeverity::Warning => pal.warning_fg,
        ProblemSeverity::Info | ProblemSeverity::Hint => pal.info_fg,
    }
}

pub(super) fn line_pieces(range: &Range<usize>, lines: &[Range<usize>]) -> Vec<Range<usize>> {
    let pieces: Vec<Range<usize>> = lines
        .iter()
        .filter_map(|line| {
            let start = range.start.max(line.start);
            let end = range.end.min(line.end);
            (start < end).then_some(start..end)
        })
        .collect();
    if !pieces.is_empty() {
        return pieces;
    }
    let inside = lines
        .iter()
        .any(|line| line.contains(&range.start) || line.end == range.start);
    std::iter::once(range.start..range.start)
        .filter(|_| inside)
        .collect()
}

fn paint_underlines(
    state: &Entity<EditorState>,
    problems: &Entity<ProblemList>,
    window: &mut Window,
    cx: &App,
) {
    let editor = state.read(cx);
    let Some(rows) = editor.visible_row_range() else {
        return;
    };
    let text = editor.text();
    let lines: Vec<Range<usize>> = rows
        .clone()
        .map(|row| text.line_start_offset(row)..text.line_end_offset(row))
        .collect();
    let pal = palette(cx);
    let mut shown: Vec<&_> = problems.read(cx).in_lines(rows).collect();
    shown.sort_by_key(|problem| std::cmp::Reverse(problem.severity));
    for problem in shown {
        let style = UnderlineStyle {
            color: Some(rgb(underline_color(problem.severity, pal)).into()),
            thickness: px(UNDERLINE_THICKNESS_PX),
            wavy: true,
        };
        for piece in line_pieces(&problem.range, &lines) {
            let Some(bounds) = editor.range_to_bounds(&piece) else {
                continue;
            };
            let width = bounds.size.width.max(px(MIN_UNDERLINE_PX));
            window.paint_underline(
                point(
                    bounds.left(),
                    bounds.bottom() - px(UNDERLINE_THICKNESS_PX * 2.0),
                ),
                width,
                &style,
            );
        }
    }
}

pub(super) fn problem_underlines(
    state: Entity<EditorState>,
    problems: Entity<ProblemList>,
) -> AnyElement {
    canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, (), window, cx| {
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                paint_underlines(&state, &problems, window, cx);
            });
        },
    )
    .absolute()
    .inset_0()
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_problem_over_several_lines_is_underlined_on_each_of_them() {
        let lines = [0..10, 11..20, 21..30];
        assert_eq!(line_pieces(&(5..25), &lines), [5..10, 11..20, 21..25]);
        let empty = line_pieces(&(12..12), &lines);
        assert_eq!((empty.len(), empty.first()), (1, Some(&(12..12))));
        let newline = line_pieces(&(10..11), &lines);
        assert_eq!((newline.len(), newline.first()), (1, Some(&(10..10))));
        assert_eq!(line_pieces(&(40..45), &lines), Vec::<Range<usize>>::new());
    }
}
