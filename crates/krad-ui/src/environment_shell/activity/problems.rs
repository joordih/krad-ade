use gpui_kit::{
    div, px, rgb, uniform_list, AnyElement, Context, FontWeight, InteractiveElement, IntoElement,
    ParentElement, SharedString, StatefulInteractiveElement, Styled, Window,
};
use krad_lsp::lsp_types::DiagnosticSeverity;
use std::ops::Range;
use std::path::{Path, PathBuf};

use super::super::language_servers::{FileRow, HostProblems, ProblemLine, ProblemRow};
use super::super::session_pane::{tab_shell, TabWidth};
use super::super::EnvironmentShell;
use super::tabs::UtilityTab;
use crate::components::{icon, IconName};
use crate::theme::{
    palette, Palette, ICON_SM, ICON_XS, LINE_SM, LINE_XS, RADIUS_XS, SPACE_1, SPACE_1_5, SPACE_2,
    TEXT_SM, TEXT_XS,
};

const PROBLEMS_TAB_ID: &str = "utilities-problems-tab";
const PROBLEMS_LIST_ID: &str = "utilities-problems";
const FILE_ROW_PREFIX: &str = "problems-file";
const PROBLEM_ROW_PREFIX: &str = "problems-row";
const PROBLEMS_LABEL: &str = "Problems";
const NO_PROBLEMS_TEXT: &str = "No problems found";
const INCOMPLETE_PREFIX: &str = "Project check incomplete:";
const INCOMPLETE_OF: &str = "of";
const INCOMPLETE_FILES: &str = "files checked";
const ROW_H: f32 = 24.0;
const PROBLEM_INDENT: f32 = 20.0;

fn severity_look(severity: DiagnosticSeverity, pal: &Palette) -> (IconName, u32) {
    match severity {
        DiagnosticSeverity::ERROR => (IconName::CancelCircle, pal.error_fg),
        DiagnosticSeverity::WARNING => (IconName::Flag, pal.warning_fg),
        _ => (IconName::Comment, pal.info_fg),
    }
}

impl EnvironmentShell {
    fn shown_problems(&self) -> Option<&HostProblems> {
        let host = &self.envs.get(&self.active)?.data.host;
        self.language_servers.diagnostics.rows().of(host)
    }

    fn problems_root(&self) -> Option<PathBuf> {
        let live = self.envs.get(&self.active)?;
        let group = live.data.group(live.selected_group?)?;
        Some(group.root.clone())
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub(super) fn problems_tab(&self, shown: UtilityTab, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let errors = self.shown_problems().map_or(0, |problems| problems.errors);
        tab_shell(
            PROBLEMS_TAB_ID,
            shown == UtilityTab::Problems,
            TabWidth::Fit,
            pal,
            cx,
        )
        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
            this.set_utility_tab(UtilityTab::Problems, cx);
        }))
        .pr(px(SPACE_2))
        .child(icon(IconName::CancelCircle, ICON_SM, pal.text_muted))
        .child(div().font_weight(FontWeight::MEDIUM).child(PROBLEMS_LABEL))
        .child(
            div()
                .text_size(px(TEXT_XS))
                .line_height(px(LINE_XS))
                .text_color(rgb(if errors > 0 {
                    pal.error_fg
                } else {
                    pal.text_muted
                }))
                .child(errors.to_string()),
        )
        .into_any_element()
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub(super) fn problems_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let count = self
            .shown_problems()
            .map_or(0, |problems| problems.rows.len());
        let body = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .px(px(SPACE_2))
            .pb(px(SPACE_2))
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .children(self.incomplete_notices(pal));
        if count == 0 {
            return body
                .child(
                    div()
                        .text_color(rgb(pal.text_muted))
                        .child(NO_PROBLEMS_TEXT),
                )
                .into_any_element();
        }
        let list = uniform_list(
            PROBLEMS_LIST_ID,
            count,
            cx.processor(|this: &mut Self, range: Range<usize>, _: &mut Window, cx| {
                this.problem_rows_in(range, cx)
            }),
        )
        .flex_1()
        .min_h_0();
        body.child(list).into_any_element()
    }

    fn problem_rows_in(&self, range: Range<usize>, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(problems) = self.shown_problems() else {
            return Vec::new();
        };
        let root = self.problems_root();
        let rows = &problems.rows;
        let file_path = |file: usize| match rows.get(file) {
            Some(ProblemRow::File(found)) => Some(found.path.as_path()),
            _ => None,
        };
        rows.get(range.clone())
            .unwrap_or_default()
            .iter()
            .zip(range)
            .filter_map(|(row, index)| match row {
                ProblemRow::File(file) => {
                    Some(Self::problem_file_row(index, file, root.as_deref(), cx))
                }
                ProblemRow::Problem(problem) => {
                    let path = file_path(problem.file)?;
                    Some(Self::problem_row(index, path, problem, cx))
                }
            })
            .collect()
    }

    fn incomplete_notices(&self, pal: &Palette) -> Vec<AnyElement> {
        let Some(host) = self.envs.get(&self.active).map(|live| &live.data.host) else {
            return Vec::new();
        };
        self.incomplete_checks(host)
            .into_iter()
            .map(|incomplete| {
                div()
                    .text_color(rgb(pal.warning_fg))
                    .child(format!(
                        "{INCOMPLETE_PREFIX} {} {} {INCOMPLETE_OF} {} {INCOMPLETE_FILES}",
                        incomplete.language, incomplete.checked, incomplete.total
                    ))
                    .into_any_element()
            })
            .collect()
    }

    fn problem_file_row(
        index: usize,
        file: &FileRow,
        root: Option<&Path>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let shown: SharedString = root
            .and_then(|root| file.path.strip_prefix(root).ok())
            .unwrap_or(&file.path)
            .to_string_lossy()
            .into_owned()
            .into();
        div()
            .id((FILE_ROW_PREFIX, index))
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SPACE_1_5))
            .h(px(ROW_H))
            .min_w_0()
            .child(icon(IconName::File, ICON_SM, pal.text_muted))
            .child(
                div()
                    .flex_none()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(pal.text_strong))
                    .child(file.name.clone()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(pal.text_muted))
                    .child(shown),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(TEXT_XS))
                    .text_color(rgb(pal.error_fg))
                    .child(file.errors.clone()),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(TEXT_XS))
                    .text_color(rgb(pal.warning_fg))
                    .child(file.warnings.clone()),
            )
            .into_any_element()
    }

    fn problem_row(
        index: usize,
        path: &Path,
        problem: &ProblemLine,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let (glyph, color) = severity_look(problem.severity, pal);
        let start = problem.start;
        let target = path.to_path_buf();
        div()
            .id((PROBLEM_ROW_PREFIX, index))
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SPACE_1_5))
            .h(px(ROW_H))
            .min_w_0()
            .pl(px(PROBLEM_INDENT))
            .pr(px(SPACE_1))
            .rounded(px(RADIUS_XS))
            .cursor_pointer()
            .hover(|style| style.bg(pal.bg_hover))
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.open_file_at(&target, Some(start), cx);
            }))
            .child(icon(glyph, ICON_XS, color))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(pal.text_default))
                    .child(problem.message.clone()),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(TEXT_XS))
                    .text_color(rgb(pal.text_muted))
                    .child(problem.place.clone()),
            )
            .into_any_element()
    }
}
