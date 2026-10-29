use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::Sizable as _;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, uniform_list, AnyElement, AppContext as _, Context, Div, Entity, FontWeight,
    HighlightStyle, InteractiveElement, IntoElement, ParentElement, Stateful,
    StatefulInteractiveElement, Styled, StyledText, Subscription, Window,
};
use krad_domain::FileOpening;
use krad_repo::{search_text, TextMatch};
use std::ops::Range;
use std::path::Path;
use std::time::Duration;

use super::super::EnvironmentShell;
use crate::components::{icon, IconName};
use crate::theme::{
    palette, tinted, Palette, ICON_SM, LINE_SM, LINE_XS, RADIUS_XS, SPACE_1, SPACE_1_5, SPACE_2,
    TEXT_SM, TEXT_XS,
};

const SEARCH_LIMIT: usize = 500;
const SEARCH_DELAY: Duration = Duration::from_millis(180);
const PLACEHOLDER: &str = "Search in files";
const SEARCHING_TEXT: &str = "Searching…";
const NO_RESULTS_TEXT: &str = "No results";
const RESULTS_ID: &str = "side-search-results";
const RESULT_PREFIX: &str = "side-search-row";
const ROW_H: f32 = 24.0;
const MATCH_INDENT: f32 = 22.0;
const LINE_NUMBER_W: f32 = 34.0;
const MATCH_WASH_ALPHA: u32 = 0x40;

#[derive(Default)]
enum SearchResults {
    #[default]
    Idle,
    Searching,
    Found(Vec<TextMatch>),
    Failed(String),
}

#[derive(Default)]
pub(in crate::environment_shell) struct SideSearch {
    field: Option<(Entity<InputState>, Subscription)>,
    query: String,
    results: SearchResults,
    generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SearchRow {
    File { first: usize, count: usize },
    Line(usize),
}

fn search_rows(matches: &[TextMatch]) -> Vec<SearchRow> {
    let mut rows = Vec::new();
    let mut index = 0;
    while let Some(first) = matches.get(index) {
        let count = matches[index..]
            .iter()
            .take_while(|found| found.path == first.path)
            .count();
        rows.push(SearchRow::File {
            first: index,
            count,
        });
        rows.extend((index..index + count).map(SearchRow::Line));
        index += count;
    }
    rows
}

fn match_range(text: &str, query: &str) -> Option<Range<usize>> {
    let lowered = text.to_lowercase();
    if lowered.len() != text.len() || query.is_empty() {
        return None;
    }
    let start = lowered.find(&query.to_lowercase())?;
    Some(start..start + query.len())
}

impl EnvironmentShell {
    pub(in crate::environment_shell) const fn side_search_open(&self) -> bool {
        self.side_search.field.is_some()
    }

    pub(in crate::environment_shell) fn toggle_side_search(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.side_search.field.take().is_some() {
            self.close_side_search(cx);
            return;
        }
        self.cancel_new_entry();
        let field = cx.new(|cx| InputState::new(window, cx).placeholder(PLACEHOLDER));
        let focus_target = field.clone();
        cx.defer_in(window, move |_, window, cx| {
            focus_target.update(cx, |field, cx| field.focus(window, cx));
        });
        let subscription = cx.subscribe_in(
            &field,
            window,
            |this: &mut Self, field, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    let query = field.read(cx).value().trim().to_owned();
                    this.search_files(query, cx);
                }
            },
        );
        self.side_search.field = Some((field, subscription));
        cx.notify();
    }

    pub(in crate::environment_shell) fn close_side_search(&mut self, cx: &mut Context<Self>) {
        self.side_search = SideSearch::default();
        cx.notify();
    }

    fn search_files(&mut self, query: String, cx: &mut Context<Self>) {
        let search = &mut self.side_search;
        if search.query == query {
            return;
        }
        search.generation += 1;
        search.query.clone_from(&query);
        let Some(root) = self
            .file_tree_root()
            .map(Path::to_path_buf)
            .filter(|_| !query.is_empty())
        else {
            self.side_search.results = SearchResults::Idle;
            cx.notify();
            return;
        };
        self.side_search.results = SearchResults::Searching;
        let generation = self.side_search.generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SEARCH_DELAY).await;
            let current = this
                .read_with(cx, |shell, _| shell.side_search.generation == generation)
                .unwrap_or_default();
            if !current {
                return;
            }
            let found = cx
                .background_spawn(async move { search_text(&root, &query, SEARCH_LIMIT) })
                .await;
            this.update(cx, |shell, cx| {
                if shell.side_search.generation != generation {
                    return;
                }
                shell.side_search.results = match found {
                    Ok(found) => SearchResults::Found(found),
                    Err(error) => SearchResults::Failed(error.to_string()),
                };
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
        cx.notify();
    }

    pub(in crate::environment_shell) fn side_search_view(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let Some((field, _)) = &self.side_search.field else {
            return div().into_any_element();
        };
        let status = match &self.side_search.results {
            SearchResults::Searching => Some(SEARCHING_TEXT.to_owned()),
            SearchResults::Found(found) if found.is_empty() => Some(NO_RESULTS_TEXT.to_owned()),
            SearchResults::Idle | SearchResults::Found(_) => None,
            SearchResults::Failed(reason) => Some(reason.clone()),
        };
        let rows = match &self.side_search.results {
            SearchResults::Found(found) => search_rows(found).len(),
            _ => 0,
        };
        let list = uniform_list(
            RESULTS_ID,
            rows,
            cx.processor(|this: &mut Self, range: Range<usize>, _: &mut Window, cx| {
                let SearchResults::Found(found) = &this.side_search.results else {
                    return Vec::new();
                };
                let rows = search_rows(found);
                rows.get(range.clone())
                    .unwrap_or_default()
                    .iter()
                    .zip(range)
                    .map(|(row, index)| this.side_search_row(*row, index, found, cx))
                    .collect::<Vec<_>>()
            }),
        )
        .flex_1()
        .min_h_0();
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(px(SPACE_1))
            .child(
                div()
                    .flex_none()
                    .px(px(SPACE_1))
                    .child(Input::new(field).small().cleanable(true)),
            )
            .when_some(status, |view, status| {
                view.child(
                    div()
                        .flex_none()
                        .px(px(SPACE_2))
                        .text_size(px(TEXT_XS))
                        .line_height(px(LINE_XS))
                        .text_color(rgb(pal.text_muted))
                        .child(status),
                )
            })
            .child(list)
            .into_any_element()
    }

    fn side_search_row(
        &self,
        row: SearchRow,
        index: usize,
        found: &[TextMatch],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let (SearchRow::File { first, .. } | SearchRow::Line(first)) = row;
        let Some(line) = found.get(first) else {
            return div().into_any_element();
        };
        let path = line.path.clone();
        let base = div()
            .id((RESULT_PREFIX, index))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_1_5))
            .w_full()
            .h(px(ROW_H))
            .px(px(SPACE_2))
            .rounded(px(RADIUS_XS))
            .cursor_pointer()
            .hover(|style| style.bg(pal.bg_hover))
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.open_file_tab(&path, FileOpening::Preview, cx);
            }));
        match row {
            SearchRow::File { count, .. } => self.search_file_cells(base, &line.path, count, pal),
            SearchRow::Line(_) => self.search_line_cells(base, line, pal),
        }
    }

    fn search_file_cells(
        &self,
        row: Stateful<Div>,
        path: &Path,
        count: usize,
        pal: &Palette,
    ) -> AnyElement {
        let name = path
            .file_name()
            .map_or_default(|name| name.to_string_lossy().into_owned());
        let folder = path
            .parent()
            .map_or_default(|parent| self.relative_to_root(parent).display().to_string());
        row.text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
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
                    .text_size(px(TEXT_XS))
                    .text_color(rgb(pal.text_muted))
                    .child(folder),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(TEXT_XS))
                    .text_color(rgb(pal.text_subtle))
                    .child(count.to_string()),
            )
            .into_any_element()
    }

    fn search_line_cells(&self, row: Stateful<Div>, line: &TextMatch, pal: &Palette) -> AnyElement {
        let mark = HighlightStyle {
            background_color: Some(tinted(pal.accent, MATCH_WASH_ALPHA).into()),
            font_weight: Some(FontWeight::SEMIBOLD),
            ..HighlightStyle::default()
        };
        let highlights: Vec<(Range<usize>, HighlightStyle)> =
            match_range(&line.text, &self.side_search.query)
                .map(|range| (range, mark))
                .into_iter()
                .collect();
        row.pl(px(MATCH_INDENT))
            .text_size(px(TEXT_XS))
            .line_height(px(LINE_XS))
            .child(
                div()
                    .flex_none()
                    .w(px(LINE_NUMBER_W))
                    .text_color(rgb(pal.text_muted))
                    .child(line.line.to_string()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_color(rgb(pal.text_default))
                    .child(StyledText::new(line.text.clone()).with_highlights(highlights)),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn found(path: &str, line: u32) -> TextMatch {
        TextMatch {
            path: PathBuf::from(path),
            line,
            text: String::new(),
        }
    }

    #[test]
    fn each_file_heads_its_matching_lines() {
        let matches = [
            found("/r/a.rs", 1),
            found("/r/a.rs", 9),
            found("/r/b.rs", 4),
        ];
        assert_eq!(
            search_rows(&matches),
            [
                SearchRow::File { first: 0, count: 2 },
                SearchRow::Line(0),
                SearchRow::Line(1),
                SearchRow::File { first: 2, count: 1 },
                SearchRow::Line(2),
            ]
        );
        assert_eq!(search_rows(&[]), []);
    }

    #[test]
    fn the_match_is_found_ignoring_case() {
        assert_eq!(match_range("let Alpha = 1;", "alpha"), Some(4..9));
        assert_eq!(match_range("nothing", "alpha"), None);
        assert_eq!(match_range("anything", ""), None);
    }
}
