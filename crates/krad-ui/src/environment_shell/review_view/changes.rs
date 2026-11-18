use gpui_kit::component::diff::DiffAnnotation;
use gpui_kit::component::ActiveTheme;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled,
};

use super::detail::RequestDetail;
use super::diff::{thread_anchors, ChangedFile};
use super::meta_line;
use super::threads::ThreadPlace;
use super::Remote;
use crate::components::{chip, icon, IconName};
use crate::environment_shell::diff_view::{
    change_decorations, id_number, mode_toggle, numbered_id, shell_element, themed_diff,
};
use crate::environment_shell::EnvironmentShell;
use crate::theme::{
    palette, row_fill, row_shadow, LINE_2XS, RADIUS_XS, SPACE_0_5, SPACE_1_5, SPACE_2, TEXT_2XS,
    TEXT_XS, TOOLBAR_H,
};

const EMPTY_PAD: f32 = 16.0;
const FILES_PAD_Y: f32 = 10.0;
const FILE_ROW_H: f32 = 28.0;
const FILE_ICON: f32 = 13.0;
const DIFF_HEADER_PAD_X: f32 = 14.0;
const FILES_ID: &str = "review-files";
const BTN_MODE: &str = "review-diff-mode";
const THREAD_PREFIX: &str = "review-diff-thread";
const FILE_PREFIX: &str = "review-file";
const ADDED_SIGN: &str = "+";
const REMOVED_SIGN: &str = "−";
const NO_CHANGES: &str = "No changes.";
const LOADING: &str = "Loading…";

impl EnvironmentShell {
    pub(super) fn changes(&self, detail: &RequestDetail, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let files = match &detail.files {
            Remote::Loading => return padded(meta_line(LOADING, pal)),
            Remote::Failed(reason) => return padded(meta_line(reason, pal)),
            Remote::Ready(files) if files.is_empty() => return padded(meta_line(NO_CHANGES, pal)),
            Remote::Ready(files) => files,
        };
        let Some((selected, shown)) = detail.shown_file() else {
            return padded(meta_line(NO_CHANGES, pal));
        };
        let rows = files
            .iter()
            .enumerate()
            .map(|(index, file)| Self::changed_file_row(index, file, index == selected, cx));
        let rail = self.review.frame.files_w.map(|width| {
            div()
                .id(FILES_ID)
                .flex()
                .flex_col()
                .flex_none()
                .w(px(width))
                .gap(px(SPACE_0_5))
                .py(px(FILES_PAD_Y))
                .px(px(SPACE_2))
                .border_r_1()
                .border_color(pal.divider)
                .overflow_y_scroll()
                .children(rows)
        });
        div()
            .flex()
            .flex_row()
            .flex_1()
            .min_h_0()
            .children(rail)
            .child(Self::changed_file_diff(selected, shown, detail, files, cx))
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
            .children(count(ADDED_SIGN, file.added, pal.diff_added))
            .children(count(REMOVED_SIGN, file.removed, pal.diff_removed))
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                if let Some(detail) = this.review.detail.as_mut() {
                    detail.select_file(index);
                }
                this.show_review_file(cx);
                cx.notify();
            }))
            .into_any_element()
    }

    fn changed_file_diff(
        index: usize,
        file: &ChangedFile,
        detail: &RequestDetail,
        files: &[ChangedFile],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let mono = cx.theme().mono_font_family.clone();
        let path = if file.change.renamed_file {
            format!("{} → {}", file.change.old_path, file.change.new_path)
        } else {
            file.change.new_path.clone()
        };
        let shown = &detail.shown;
        let body = if let Some(reason) = &shown.failure {
            padded(meta_line(reason, pal))
        } else {
            let threads = match &detail.discussions {
                Remote::Ready(discussions) => {
                    thread_anchors(files, discussions, index, &shown.path)
                }
                Remote::Loading | Remote::Failed(_) => Vec::new(),
            };
            let shell = cx.entity().downgrade();
            themed_diff(&shown.state)
                .header_visible(false)
                .decorations(change_decorations(&shown.path, &shown.runs, pal).collect::<Vec<_>>())
                .annotations(threads.into_iter().map(|anchor| {
                    DiffAnnotation::line(
                        numbered_id(THREAD_PREFIX, anchor.discussion as u64),
                        anchor.position,
                    )
                }))
                .render_annotation(move |annotation, _, cx| {
                    let discussion = id_number(annotation.id())
                        .and_then(|discussion| usize::try_from(discussion).ok());
                    shell_element(&shell, cx, |this, cx| {
                        let detail = this.review.detail.as_ref()?;
                        let Remote::Ready(discussions) = &detail.discussions else {
                            return None;
                        };
                        let discussion = discussion?;
                        Some(
                            div()
                                .font_family(cx.theme().font_family.clone())
                                .child(Self::thread(
                                    discussion,
                                    discussions.get(discussion)?,
                                    detail,
                                    ThreadPlace::Diff,
                                    cx,
                                ))
                                .into_any_element(),
                        )
                    })
                })
                .into_any_element()
        };
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
                            .font_family(mono)
                            .text_size(px(TEXT_XS))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(pal.text_strong))
                            .child(path),
                    )
                    .child(chip(
                        SharedString::from(file.source.label(detail.words)),
                        pal,
                    ))
                    .child(mode_toggle(BTN_MODE, &shown.state, cx)),
            )
            .child(div().flex().flex_col().flex_1().min_h_0().child(body))
            .into_any_element()
    }
}

fn padded(content: AnyElement) -> AnyElement {
    div().p(px(EMPTY_PAD)).child(content).into_any_element()
}
