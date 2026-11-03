use gpui_kit::{
    div, px, rgb, AnyElement, Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use krad_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};
use std::path::{Path, PathBuf};

use super::super::language_servers::severity_of;
use super::super::session_pane::tab_shell;
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
const NO_PROBLEMS_TEXT: &str = "No problems in the open files";
const ROW_H: f32 = 24.0;
const PROBLEM_INDENT: f32 = 20.0;
const LINE_BASE: u32 = 1;

struct FileProblems {
    path: PathBuf,
    shown: String,
    problems: Vec<Diagnostic>,
}

fn severity_look(severity: DiagnosticSeverity, pal: &Palette) -> (IconName, u32) {
    match severity {
        DiagnosticSeverity::ERROR => (IconName::CancelCircle, pal.error_fg),
        DiagnosticSeverity::WARNING => (IconName::Flag, pal.warning_fg),
        _ => (IconName::Comment, pal.info_fg),
    }
}

fn count_of(problems: &[Diagnostic], severity: DiagnosticSeverity) -> usize {
    problems
        .iter()
        .filter(|problem| severity_of(problem) == severity)
        .count()
}

impl EnvironmentShell {
    fn file_problems(&self) -> Vec<FileProblems> {
        let root = self.envs.get(&self.active).and_then(|live| {
            let group = live.data.group(live.selected_group?)?;
            Some(group.root.clone())
        });
        let host = self
            .envs
            .get(&self.active)
            .map(|live| live.data.host.clone());
        let store = &self.language_servers.diagnostics;
        store
            .files()
            .into_iter()
            .filter(|document| host.as_ref() == Some(&document.host))
            .map(|document| {
                let shown = root
                    .as_deref()
                    .and_then(|root| document.path.strip_prefix(root).ok())
                    .unwrap_or(&document.path)
                    .to_string_lossy()
                    .into_owned();
                FileProblems {
                    problems: store.of(&document),
                    path: document.path,
                    shown,
                }
            })
            .collect()
    }

    pub(super) fn problems_tab(&self, shown: UtilityTab, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let errors: usize = self
            .file_problems()
            .iter()
            .map(|file| count_of(&file.problems, DiagnosticSeverity::ERROR))
            .sum();
        tab_shell(PROBLEMS_TAB_ID, shown == UtilityTab::Problems, pal, cx)
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

    pub(super) fn problems_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let files = self.file_problems();
        let list = div()
            .id(PROBLEMS_LIST_ID)
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .px(px(SPACE_2))
            .pb(px(SPACE_2))
            .overflow_y_scroll()
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM));
        if files.is_empty() {
            return list
                .text_color(rgb(pal.text_muted))
                .child(NO_PROBLEMS_TEXT)
                .into_any_element();
        }
        let mut rows = Vec::new();
        let mut index = 0;
        for (file_index, file) in files.into_iter().enumerate() {
            rows.push(Self::problem_file_row(file_index, &file, cx));
            for problem in &file.problems {
                rows.push(Self::problem_row(index, &file.path, problem, cx));
                index += 1;
            }
        }
        list.children(rows).into_any_element()
    }

    fn problem_file_row(index: usize, file: &FileProblems, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let name = Path::new(&file.shown).file_name().map_or_else(
            || file.shown.clone(),
            |name| name.to_string_lossy().into_owned(),
        );
        let errors = count_of(&file.problems, DiagnosticSeverity::ERROR);
        let warnings = count_of(&file.problems, DiagnosticSeverity::WARNING);
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
                    .child(name),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(pal.text_muted))
                    .child(file.shown.clone()),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(TEXT_XS))
                    .text_color(rgb(pal.error_fg))
                    .child(errors.to_string()),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(TEXT_XS))
                    .text_color(rgb(pal.warning_fg))
                    .child(warnings.to_string()),
            )
            .into_any_element()
    }

    fn problem_row(
        index: usize,
        path: &Path,
        problem: &Diagnostic,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let (glyph, color) = severity_look(severity_of(problem), pal);
        let start = problem.range.start;
        let place = format!("{}:{}", start.line + LINE_BASE, start.character + LINE_BASE);
        let target = path.to_path_buf();
        let message = problem
            .message
            .lines()
            .next()
            .unwrap_or_default()
            .to_owned();
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
                    .child(message),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(TEXT_XS))
                    .text_color(rgb(pal.text_muted))
                    .child(place),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_lsp::lsp_types::{Position, Range};

    #[test]
    fn counts_split_errors_from_warnings() {
        let with = |severity| Diagnostic {
            range: Range::new(Position::new(0, 0), Position::new(0, 1)),
            severity,
            ..Diagnostic::default()
        };
        let problems = [
            with(Some(DiagnosticSeverity::ERROR)),
            with(None),
            with(Some(DiagnosticSeverity::WARNING)),
        ];
        assert_eq!(count_of(&problems, DiagnosticSeverity::ERROR), 2);
        assert_eq!(count_of(&problems, DiagnosticSeverity::WARNING), 1);
    }
}
