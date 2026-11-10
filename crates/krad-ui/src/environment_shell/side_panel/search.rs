use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::Sizable as _;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, uniform_list, AnyElement, AppContext as _, Context, Entity, IntoElement,
    ParentElement, Styled, Subscription, Window,
};
use krad_repo::{parse_globs, search_project, ProjectMatches, ProjectQuery, ReplaceUndo};
use std::ops::Range;
use std::path::Path;
use std::time::Duration;

use super::super::EnvironmentShell;
use crate::theme::{palette, LINE_XS, SPACE_1, SPACE_2, TEXT_XS};

mod apply;
mod options;
mod preview;
mod rows;

use preview::{search_rows, SearchRow};

const SEARCH_LIMIT: usize = 2000;
const SEARCH_DELAY: Duration = Duration::from_millis(180);
const FIND_PLACEHOLDER: &str = "Search in files";
const REPLACE_PLACEHOLDER: &str = "Replace";
const INCLUDE_PLACEHOLDER: &str = "Files to include, as *.rs, src/**";
const EXCLUDE_PLACEHOLDER: &str = "Files to exclude, as target, *.lock";
const SEARCHING_TEXT: &str = "Searching…";
const NO_RESULTS_TEXT: &str = "No results";
const RESULTS_ID: &str = "side-search-results";

#[derive(Default)]
enum SearchResults {
    #[default]
    Idle,
    Searching,
    Found {
        matches: ProjectMatches,
        rows: Vec<SearchRow>,
    },
    Failed(String),
}

struct SearchFields {
    find: Entity<InputState>,
    replace: Entity<InputState>,
    include: Entity<InputState>,
    exclude: Entity<InputState>,
    _changes: Vec<Subscription>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Drawer {
    #[default]
    Closed,
    Open,
}

impl Drawer {
    const fn toggled(self) -> Self {
        match self {
            Self::Closed => Self::Open,
            Self::Open => Self::Closed,
        }
    }

    const fn is_open(self) -> bool {
        matches!(self, Self::Open)
    }
}

#[derive(Default)]
pub(in crate::environment_shell) struct SideSearch {
    fields: Option<SearchFields>,
    query: ProjectQuery,
    replacement: String,
    replace_drawer: Drawer,
    filters_drawer: Drawer,
    results: SearchResults,
    replacing: bool,
    confirm_replace: Option<apply::ReplacePlan>,
    last_replace: Option<Vec<ReplaceUndo>>,
    generation: u64,
}

impl SideSearch {
    fn replacement(&self) -> Option<&str> {
        self.replace_drawer
            .is_open()
            .then_some(self.replacement.as_str())
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) const fn side_search_open(&self) -> bool {
        self.side_search.fields.is_some()
    }

    pub(in crate::environment_shell) fn on_find_in_files(
        &mut self,
        _: &crate::actions::FindInFiles,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.side_search.fields.is_none() {
            self.toggle_side_search(window, cx);
        }
    }

    pub(in crate::environment_shell) fn toggle_side_search(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.side_search.fields.take().is_some() {
            self.close_side_search(cx);
            return;
        }
        self.cancel_entry_input(cx);
        let mut changes = Vec::new();
        let mut field = |placeholder: &'static str, cx: &mut Context<Self>| {
            let field = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
            changes.push(cx.subscribe_in(
                &field,
                window,
                |this: &mut Self, _, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.read_search_fields(cx);
                    }
                },
            ));
            field
        };
        let find = field(FIND_PLACEHOLDER, cx);
        let replace = field(REPLACE_PLACEHOLDER, cx);
        let include = field(INCLUDE_PLACEHOLDER, cx);
        let exclude = field(EXCLUDE_PLACEHOLDER, cx);
        let focus_target = find.clone();
        cx.defer_in(window, move |_, window, cx| {
            focus_target.update(cx, |field, cx| field.focus(window, cx));
        });
        self.side_search.fields = Some(SearchFields {
            find,
            replace,
            include,
            exclude,
            _changes: changes,
        });
        cx.notify();
    }

    pub(in crate::environment_shell) fn close_side_search(&mut self, cx: &mut Context<Self>) {
        self.side_search = SideSearch::default();
        cx.notify();
    }

    fn read_search_fields(&mut self, cx: &mut Context<Self>) {
        let Some(fields) = &self.side_search.fields else {
            return;
        };
        let value = |field: &Entity<InputState>| field.read(cx).value().to_string();
        let query = ProjectQuery {
            pattern: value(&fields.find).trim().to_owned(),
            include: parse_globs(&value(&fields.include)),
            exclude: parse_globs(&value(&fields.exclude)),
            ..self.side_search.query.clone()
        };
        self.side_search.replacement = value(&fields.replace);
        self.search_project_files(query, cx);
        cx.notify();
    }

    fn search_project_files(&mut self, query: ProjectQuery, cx: &mut Context<Self>) {
        if self.side_search.query == query {
            return;
        }
        self.side_search.query = query;
        self.rerun_side_search(cx);
    }

    fn rerun_side_search(&mut self, cx: &mut Context<Self>) {
        let search = &mut self.side_search;
        search.generation += 1;
        search.confirm_replace = None;
        let query = search.query.clone();
        let Some(root) = self
            .file_tree_root()
            .map(Path::to_path_buf)
            .filter(|_| !query.pattern.is_empty())
        else {
            self.side_search.results = SearchResults::Idle;
            cx.notify();
            return;
        };
        self.side_search.results = SearchResults::Searching;
        let generation = self.side_search.generation;
        let link = self.active_link();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SEARCH_DELAY).await;
            let current = this
                .read_with(cx, |shell, _| shell.side_search.generation == generation)
                .unwrap_or_default();
            if !current {
                return;
            }
            let found = cx
                .background_spawn(async move { search_project(&link, &root, &query, SEARCH_LIMIT) })
                .await;
            this.update(cx, |shell, cx| {
                if shell.side_search.generation != generation {
                    return;
                }
                shell.side_search.results = match found {
                    Ok(matches) => SearchResults::Found {
                        rows: search_rows(&matches),
                        matches,
                    },
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
        let Some(fields) = &self.side_search.fields else {
            return div().into_any_element();
        };
        let status = match &self.side_search.results {
            SearchResults::Searching => Some(SEARCHING_TEXT.to_owned()),
            SearchResults::Found { matches, .. } if matches.files.is_empty() => {
                Some(NO_RESULTS_TEXT.to_owned())
            }
            SearchResults::Found { matches, .. } => Some(options::summary(matches)),
            SearchResults::Idle => None,
            SearchResults::Failed(reason) => Some(reason.clone()),
        };
        let rows = match &self.side_search.results {
            SearchResults::Found { rows, .. } => rows.len(),
            _ => 0,
        };
        let list = uniform_list(
            RESULTS_ID,
            rows,
            cx.processor(|this: &mut Self, range: Range<usize>, _: &mut Window, cx| {
                this.side_search_rows(range, cx)
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
            .child(self.search_find_row(&fields.find, cx))
            .when(self.side_search.replace_drawer.is_open(), |view| {
                view.child(self.search_replace_row(&fields.replace, cx))
            })
            .when(self.side_search.filters_drawer.is_open(), |view| {
                view.child(search_field(&fields.include))
                    .child(search_field(&fields.exclude))
            })
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
}

fn search_field(field: &Entity<InputState>) -> AnyElement {
    div()
        .flex_none()
        .px(px(SPACE_1))
        .child(Input::new(field).small().cleanable(true))
        .into_any_element()
}
