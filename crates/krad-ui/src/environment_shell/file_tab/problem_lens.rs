use gpui_kit::component::input::{EditorState, RopeExt as _};
use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::{
    canvas, point, px, rgb, AnyElement, App, Bounds, Entity, Hsla, IntoElement, Pixels,
    SharedString, Styled, TextAlign, TextRun, TruncateFrom, Window,
};
use krad_domain::{ProblemLens, ProblemSeverity};
use std::collections::BTreeMap;

use super::problem_list::ProblemList;
use crate::theme::{palette, Palette, TEXT_SM};

const LENS_GAP_PX: f32 = 24.0;
const LENS_MIN_WIDTH_PX: f32 = 48.0;
const LENS_DOT: &str = "● ";
const ELLIPSIS: &str = "…";
const MORE_PREFIX: &str = "  +";
const LENS_ALPHA: f32 = 0.75;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LensLine {
    pub(super) line: usize,
    pub(super) severity: ProblemSeverity,
    pub(super) message: SharedString,
    pub(super) more: usize,
}

impl LensLine {
    fn label(&self) -> String {
        let first = self.message.lines().next().unwrap_or_default().trim();
        match self.more {
            0 => format!("{LENS_DOT}{first}"),
            more => format!("{LENS_DOT}{first}{MORE_PREFIX}{more}"),
        }
    }
}

pub(super) fn lens_lines<'a>(
    problems: impl IntoIterator<Item = (usize, ProblemSeverity, &'a SharedString)>,
    lens: ProblemLens,
) -> Vec<LensLine> {
    let mut lines: BTreeMap<usize, LensLine> = BTreeMap::new();
    for (line, severity, message) in problems {
        if !lens.shows(severity) {
            continue;
        }
        lines
            .entry(line)
            .and_modify(|shown| {
                shown.more += 1;
                if severity < shown.severity {
                    shown.severity = severity;
                    shown.message = message.clone();
                }
            })
            .or_insert_with(|| LensLine {
                line,
                severity,
                message: message.clone(),
                more: 0,
            });
    }
    lines.into_values().collect()
}

fn severity_color(severity: ProblemSeverity, pal: &Palette) -> Hsla {
    let color = match severity {
        ProblemSeverity::Error => pal.error_fg,
        ProblemSeverity::Warning => pal.warning_fg,
        ProblemSeverity::Info | ProblemSeverity::Hint => pal.info_fg,
    };
    Hsla::from(rgb(color)).opacity(LENS_ALPHA)
}

fn visible_lens_lines(
    state: &EditorState,
    problems: &ProblemList,
    lens: ProblemLens,
) -> Vec<LensLine> {
    let Some(rows) = state.visible_row_range() else {
        return Vec::new();
    };
    let shown = problems
        .in_lines(rows)
        .map(|problem| (problem.line, problem.severity, &problem.message));
    lens_lines(shown, lens)
}

fn paint_lens(
    state: &Entity<EditorState>,
    problems: &Entity<ProblemList>,
    lens: ProblemLens,
    area: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let editor = state.read(cx);
    let Some(line_height) = editor.line_height() else {
        return;
    };
    let right = editor.input_bounds().right().min(area.right()) - Scrollbar::width();
    let shown: Vec<(LensLine, Bounds<Pixels>)> =
        visible_lens_lines(editor, problems.read(cx), lens)
            .into_iter()
            .filter_map(|lensed| {
                let end = editor.text().line_end_offset(lensed.line);
                editor
                    .range_to_bounds(&(end..end))
                    .map(|bounds| (lensed, bounds))
            })
            .collect();
    let pal = palette(cx);
    let style = window.text_style();
    let font = style.font();
    let font_size = px(TEXT_SM);
    for (lensed, bounds) in shown {
        let left = bounds.right() + px(LENS_GAP_PX);
        let width = right - left;
        if width < px(LENS_MIN_WIDTH_PX) || bounds.top() < area.top() {
            continue;
        }
        let label = SharedString::from(lensed.label());
        let run = TextRun {
            len: label.len(),
            font: font.clone(),
            color: severity_color(lensed.severity, pal),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = [run];
        let (label, runs) = window
            .text_system()
            .line_wrapper(font.clone(), font_size)
            .truncate_line(label, width, ELLIPSIS, &runs, TruncateFrom::End);
        let shaped = window
            .text_system()
            .shape_line(label, font_size, &runs, None);
        shaped
            .paint(
                point(left, bounds.top()),
                line_height,
                TextAlign::Left,
                None,
                window,
                cx,
            )
            .unwrap_or_default();
    }
}

pub(super) fn problem_lens(
    state: Entity<EditorState>,
    problems: Entity<ProblemList>,
    lens: ProblemLens,
) -> AnyElement {
    canvas(
        |_, _, _| (),
        move |bounds, (), window, cx| {
            window.with_content_mask(Some(gpui_kit::ContentMask { bounds }), |window| {
                paint_lens(&state, &problems, lens, bounds, window, cx);
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
    fn a_line_shows_its_most_severe_shown_problem_and_counts_the_rest() {
        let (unused, missing, style) = (
            SharedString::from("unused import"),
            SharedString::from("cannot find value `x`\nhelp: a local exists"),
            SharedString::from("line too long"),
        );
        let problems = [
            (3, ProblemSeverity::Warning, &unused),
            (3, ProblemSeverity::Error, &missing),
            (3, ProblemSeverity::Hint, &style),
            (7, ProblemSeverity::Hint, &style),
        ];
        let shown = lens_lines(problems, ProblemLens::ErrorsAndWarnings);
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].severity, ProblemSeverity::Error);
        assert_eq!(shown[0].label(), "● cannot find value `x`  +1");
        assert_eq!(lens_lines(problems, ProblemLens::Off), Vec::new());
        assert_eq!(lens_lines(problems, ProblemLens::All).len(), 2);
    }
}
