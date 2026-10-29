use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{div, px, rgb, AnyElement, IntoElement, ParentElement, SharedString, Styled};
use krad_domain::{EditLine, FileEdit, LineChange};

use crate::theme::{tinted, Palette, LINE_XS, RADIUS_SM, SPACE_1, SPACE_2, TEXT_XS};

const SHOWN_LINES: usize = 24;
const WASH_ALPHA: u32 = 0x22;
const SIGN_W: f32 = 14.0;
const ADDED_SIGN: &str = "+";
const REMOVED_SIGN: &str = "-";
const MORE_SUFFIX: &str = "more lines";
const GAP_MARK: &str = "⋯";
const NUMBER_W: f32 = 40.0;

pub(super) fn edit_counts(edit: &FileEdit) -> String {
    let (added, removed) = edit.counts();
    format!("{ADDED_SIGN}{added} {REMOVED_SIGN}{removed}")
}

fn line_number(line: &EditLine) -> Option<u32> {
    line.new_line.or(line.old_line)
}

fn edit_line(line: &EditLine, numbered: bool, pal: &Palette) -> AnyElement {
    if line.change == LineChange::Gap {
        return div()
            .px(px(SPACE_2))
            .text_color(rgb(pal.text_muted))
            .child(GAP_MARK)
            .into_any_element();
    }
    let (wash, sign, sign_color) = match line.change {
        LineChange::Added => (
            Some(tinted(pal.success_fg, WASH_ALPHA)),
            ADDED_SIGN,
            pal.success_fg,
        ),
        LineChange::Removed => (
            Some(tinted(pal.error_fg, WASH_ALPHA)),
            REMOVED_SIGN,
            pal.error_fg,
        ),
        LineChange::Kept | LineChange::Gap => (None, "", pal.text_muted),
    };
    div()
        .flex()
        .flex_row()
        .flex_none()
        .px(px(SPACE_1))
        .when_some(wash, Styled::bg)
        .text_color(rgb(if line.change == LineChange::Kept {
            pal.text_muted
        } else {
            pal.text_strong
        }))
        .when(numbered, |row| {
            row.child(
                div()
                    .flex_none()
                    .w(px(NUMBER_W))
                    .pr(px(SPACE_2))
                    .text_right()
                    .text_color(rgb(pal.text_muted))
                    .child(line_number(line).map_or_default(|number| number.to_string())),
            )
        })
        .child(
            div()
                .flex_none()
                .w(px(SIGN_W))
                .text_color(rgb(sign_color))
                .child(sign),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .child(SharedString::from(line.text.clone())),
        )
        .into_any_element()
}

pub(in crate::environment_shell) fn edit_diff(
    edit: &FileEdit,
    mono: &SharedString,
    pal: &Palette,
) -> AnyElement {
    let hidden = edit.lines.len().saturating_sub(SHOWN_LINES);
    let numbered = edit.lines.iter().any(|line| line_number(line).is_some());
    div()
        .flex()
        .flex_col()
        .mt(px(SPACE_1))
        .py(px(SPACE_1))
        .rounded(px(RADIUS_SM))
        .overflow_hidden()
        .bg(rgb(pal.bg_surface))
        .font_family(mono.clone())
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .children(
            edit.lines
                .iter()
                .take(SHOWN_LINES)
                .map(|line| edit_line(line, numbered, pal)),
        )
        .when(hidden > 0, |diff| {
            diff.child(
                div()
                    .px(px(SPACE_2))
                    .text_color(rgb(pal.text_muted))
                    .child(format!("… {hidden} {MORE_SUFFIX}")),
            )
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::line_diff;

    #[test]
    fn an_edited_file_counts_its_added_and_removed_lines() {
        let edit = FileEdit {
            path: "a".to_owned(),
            lines: line_diff("x\ny", "x\nz\nw"),
        };
        assert_eq!(edit_counts(&edit), "+2 -1");
    }
}
