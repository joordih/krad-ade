use gpui_kit::component::ActiveTheme;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled,
};
use krad_domain::review::Discussion;
use std::collections::HashMap;

use super::detail::RequestDetail;
use super::diff::{anchor_line, ChangedFile, DiffLine, LineKind};
use super::threads::ThreadPlace;
use super::Remote;
use super::{meta_line, WASH_ALPHA};
use crate::components::{chip, icon, IconName};
use crate::environment_shell::EnvironmentShell;
use crate::theme::{
    palette, row_fill, row_shadow, tinted, Palette, LINE_2XS, RADIUS_XS, SPACE_0_5, SPACE_1_5,
    SPACE_2, SPACE_3, TEXT_2XS, TEXT_XS, TOOLBAR_H,
};

const FILES_W: f32 = 236.0;
const EMPTY_PAD: f32 = 16.0;
const FILES_PAD_Y: f32 = 10.0;
const FILE_ROW_H: f32 = 28.0;
const FILE_ICON: f32 = 13.0;
const DIFF_HEADER_PAD_X: f32 = 14.0;
const DIFF_LINE_H: f32 = 20.0;
const NUMBER_W: f32 = 40.0;
const SIGN_W: f32 = 18.0;
const THREAD_INSET_LEFT: f32 = NUMBER_W * 2.0 + SIGN_W;
const THREAD_INSET_RIGHT: f32 = 16.0;
const THREAD_INSET_BOTTOM: f32 = 10.0;
const MAX_DIFF_LINES: usize = 400;
const FILES_ID: &str = "review-files";
const DIFF_ID: &str = "review-diff";
const FILE_PREFIX: &str = "review-file";
const ADDED_SIGN: &str = "+";
const REMOVED_SIGN: &str = "−";
const NO_CHANGES: &str = "No changes.";
const LOADING: &str = "Loading…";

impl EnvironmentShell {
    pub(super) fn changes(detail: &RequestDetail, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let files = match &detail.files {
            Remote::Loading => return padded(meta_line(LOADING, pal)),
            Remote::Failed(reason) => return padded(meta_line(reason, pal)),
            Remote::Ready(files) if files.is_empty() => return padded(meta_line(NO_CHANGES, pal)),
            Remote::Ready(files) => files,
        };
        let selected = detail.selected_file.min(files.len() - 1);
        let rows = files
            .iter()
            .enumerate()
            .map(|(index, file)| Self::changed_file_row(index, file, index == selected, cx));
        div()
            .flex()
            .flex_row()
            .flex_1()
            .min_h_0()
            .child(
                div()
                    .id(FILES_ID)
                    .flex()
                    .flex_col()
                    .flex_none()
                    .w(px(FILES_W))
                    .gap(px(SPACE_0_5))
                    .py(px(FILES_PAD_Y))
                    .px(px(SPACE_2))
                    .border_r_1()
                    .border_color(pal.divider)
                    .overflow_y_scroll()
                    .children(rows),
            )
            .child(Self::changed_file_diff(&files[selected], detail, cx))
            .into_any_element()
    }

    fn changed_file_row(
        index: usize,
        file: &ChangedFile,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let mono = cx.theme().mono_font_family.clone();
        let count = |sign: &str, count: usize, color: u32| {
            (count > 0).then(|| {
                div()
                    .flex_none()
                    .font_family(mono.clone())
                    .text_size(px(TEXT_2XS))
                    .line_height(px(LINE_2XS))
                    .text_color(rgb(color))
                    .child(format!("{sign}{count}"))
            })
        };
        div()
            .id((FILE_PREFIX, index as u64))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_1_5))
            .h(px(FILE_ROW_H))
            .px(px(SPACE_2))
            .rounded(px(RADIUS_XS))
            .cursor_pointer()
            .text_size(px(TEXT_XS))
            .text_color(rgb(pal.text_strong))
            .when(selected, |row| {
                row.bg(row_fill(pal, cx)).shadow(row_shadow(pal, cx))
            })
            .when(!selected, |row| row.hover(|style| style.bg(pal.bg_hover)))
            .child(icon(IconName::File, FILE_ICON, pal.text_muted))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(file.name().to_owned()),
            )
            .children(count(ADDED_SIGN, file.added, pal.success_fg))
            .children(count(REMOVED_SIGN, file.removed, pal.error_fg))
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                if let Some(detail) = this.review.detail.as_mut() {
                    detail.selected_file = index;
                }
                cx.notify();
            }))
            .into_any_element()
    }

    fn changed_file_diff(
        file: &ChangedFile,
        detail: &RequestDetail,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let mono = cx.theme().mono_font_family.clone();
        let path = if file.change.renamed_file {
            format!("{} → {}", file.change.old_path, file.change.new_path)
        } else {
            file.change.new_path.clone()
        };
        let threads = threads_by_line(file, detail);
        let mut rows: Vec<AnyElement> = Vec::new();
        for (index, line) in file.lines.iter().take(MAX_DIFF_LINES).enumerate() {
            rows.push(diff_row(line, pal));
            for (thread_index, discussion) in threads.get(&index).into_iter().flatten() {
                rows.push(
                    div()
                        .mt(px(SPACE_2))
                        .mb(px(THREAD_INSET_BOTTOM))
                        .ml(px(THREAD_INSET_LEFT))
                        .mr(px(THREAD_INSET_RIGHT))
                        .font_family(cx.theme().font_family.clone())
                        .child(Self::thread(
                            *thread_index,
                            discussion,
                            detail,
                            ThreadPlace::Diff,
                            cx,
                        ))
                        .into_any_element(),
                );
            }
        }
        let hidden = file.lines.len().saturating_sub(MAX_DIFF_LINES);
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_none()
                    .items_center()
                    .gap(px(SPACE_2))
                    .h(px(TOOLBAR_H))
                    .px(px(DIFF_HEADER_PAD_X))
                    .border_b_1()
                    .border_color(pal.divider)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_family(mono.clone())
                            .text_size(px(TEXT_XS))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(pal.text_strong))
                            .child(path),
                    )
                    .child(chip(
                        SharedString::from(file.source.label(detail.words)),
                        pal,
                    )),
            )
            .child(
                div()
                    .id(DIFF_ID)
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .font_family(mono)
                    .text_size(px(TEXT_XS))
                    .line_height(px(DIFF_LINE_H))
                    .children(rows)
                    .children((hidden > 0).then(|| {
                        div()
                            .p(px(SPACE_3))
                            .child(meta_line(&format!("… {hidden} more lines"), pal))
                    })),
            )
            .into_any_element()
    }
}

fn threads_by_line<'a>(
    file: &ChangedFile,
    detail: &'a RequestDetail,
) -> HashMap<usize, Vec<(usize, &'a Discussion)>> {
    let mut threads: HashMap<usize, Vec<(usize, &Discussion)>> = HashMap::new();
    let Remote::Ready(discussions) = &detail.discussions else {
        return threads;
    };
    for (index, discussion) in discussions.iter().enumerate() {
        let Some(position) = discussion
            .notes
            .first()
            .and_then(|note| note.position.as_ref())
        else {
            continue;
        };
        if !file.holds(position) {
            continue;
        }
        if let Some(line) = anchor_line(&file.lines, position) {
            threads.entry(line).or_default().push((index, discussion));
        }
    }
    threads
}

pub(in crate::environment_shell) fn diff_row(line: &DiffLine, pal: &Palette) -> AnyElement {
    let (background, sign, sign_color) = match line.kind {
        LineKind::Added => (
            Some(tinted(pal.success_fg, WASH_ALPHA)),
            ADDED_SIGN,
            pal.success_fg,
        ),
        LineKind::Removed => (
            Some(tinted(pal.error_fg, WASH_ALPHA)),
            REMOVED_SIGN,
            pal.error_fg,
        ),
        LineKind::Context | LineKind::Hunk => (None, "", pal.text_muted),
    };
    let text_color = if line.kind == LineKind::Hunk {
        pal.text_muted
    } else {
        pal.text_strong
    };
    let number = |value: Option<u32>| {
        div()
            .flex_none()
            .w(px(NUMBER_W))
            .pr(px(SPACE_2))
            .text_right()
            .text_color(rgb(pal.text_muted))
            .child(value.map_or_default(|value| value.to_string()))
    };
    div()
        .flex()
        .flex_row()
        .flex_none()
        .when(line.kind == LineKind::Hunk, |row| row.bg(rgb(pal.bg_panel)))
        .when_some(background, gpui_kit::Styled::bg)
        .text_color(rgb(text_color))
        .child(number(line.old))
        .child(number(line.new))
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
                .child(line.text.clone()),
        )
        .into_any_element()
}

fn padded(content: AnyElement) -> AnyElement {
    div().p(px(EMPTY_PAD)).child(content).into_any_element()
}
