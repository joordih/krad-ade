use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::Sizable as _;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{div, px, rgb, AnyElement, Context, Entity, IntoElement, ParentElement, Styled};
use krad_repo::{CaseMatching, PatternSyntax, ProjectMatches, WordMatching};

use super::super::super::EnvironmentShell;
use crate::components::{Button, ButtonVariant, IconName};
use crate::theme::{palette, LINE_XS, SPACE_0_5, SPACE_1, TEXT_XS};

const BTN_CASE: &str = "side-search-case";
const BTN_WORD: &str = "side-search-word";
const BTN_REGEX: &str = "side-search-regex";
const BTN_REPLACE_DRAWER: &str = "side-search-replace-drawer";
const BTN_FILTERS_DRAWER: &str = "side-search-filters-drawer";
const BTN_REPLACE_ALL: &str = "side-search-replace-all";
const BTN_CONFIRM_REPLACE: &str = "side-search-confirm-replace";
const BTN_CANCEL_REPLACE: &str = "side-search-cancel-replace";
const BTN_UNDO_REPLACE: &str = "side-search-undo-replace";
const CONFIRM_REPLACE_LABEL: &str = "Replace";
const CANCEL_REPLACE_LABEL: &str = "Cancel";
const UNDO_REPLACE_LABEL: &str = "Undo replace";
const TOOLTIP_UNDO_REPLACE: &str = "Restore the files as they were before the last replacement";
const CASE_LABEL: &str = "Aa";
const WORD_LABEL: &str = "ab";
const REGEX_LABEL: &str = ".*";
const TOOLTIP_CASE: &str = "Match case";
const TOOLTIP_WORD: &str = "Match whole word";
const TOOLTIP_REGEX: &str = "Use regular expression";
const TOOLTIP_REPLACE_DRAWER: &str = "Toggle replace";
const TOOLTIP_FILTERS_DRAWER: &str = "Toggle files to include and exclude";
const TOOLTIP_REPLACE_ALL: &str = "Replace in all files";
const MORE_RESULTS_SUFFIX: &str = ", more not shown";
const ONE_RESULT: &str = "result";
const MANY_RESULTS: &str = "results";
const ONE_FILE: &str = "file";
const MANY_FILES: &str = "files";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SearchOption {
    Case,
    Word,
    Regex,
}

const fn variant(on: bool) -> ButtonVariant {
    if on {
        ButtonVariant::Secondary
    } else {
        ButtonVariant::Ghost
    }
}

pub(super) fn summary(matches: &ProjectMatches) -> String {
    let count = matches.match_count();
    let files = matches.files.len();
    let results = if count == 1 { ONE_RESULT } else { MANY_RESULTS };
    let in_files = if files == 1 { ONE_FILE } else { MANY_FILES };
    let more = if matches.truncated {
        MORE_RESULTS_SUFFIX
    } else {
        ""
    };
    format!("{count} {results} in {files} {in_files}{more}")
}

impl EnvironmentShell {
    pub(super) fn search_find_row(
        &self,
        find: &Entity<InputState>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let query = &self.side_search.query;
        let search = &self.side_search;
        let option = |id: &'static str, label: &'static str, tooltip: &'static str, on, which| {
            Button::new(id)
                .label(label)
                .small()
                .tooltip(tooltip)
                .variant(variant(on))
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.toggle_search_option(which, cx);
                }))
        };
        let case = option(
            BTN_CASE,
            CASE_LABEL,
            TOOLTIP_CASE,
            query.case == CaseMatching::Sensitive,
            SearchOption::Case,
        );
        let word = option(
            BTN_WORD,
            WORD_LABEL,
            TOOLTIP_WORD,
            query.word == WordMatching::WholeWord,
            SearchOption::Word,
        );
        let regex = option(
            BTN_REGEX,
            REGEX_LABEL,
            TOOLTIP_REGEX,
            query.syntax == PatternSyntax::Regex,
            SearchOption::Regex,
        );
        let replace_drawer = Button::new(BTN_REPLACE_DRAWER)
            .icon(IconName::Exchange)
            .small()
            .tooltip(TOOLTIP_REPLACE_DRAWER)
            .variant(variant(search.replace_drawer.is_open()))
            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                this.side_search.replace_drawer = this.side_search.replace_drawer.toggled();
                cx.notify();
            }));
        let filters_drawer = Button::new(BTN_FILTERS_DRAWER)
            .icon(IconName::More)
            .small()
            .tooltip(TOOLTIP_FILTERS_DRAWER)
            .variant(variant(search.filters_drawer.is_open()))
            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                this.side_search.filters_drawer = this.side_search.filters_drawer.toggled();
                cx.notify();
            }));
        field_row(find)
            .child(case)
            .child(word)
            .child(regex)
            .child(replace_drawer)
            .child(filters_drawer)
            .into_any_element()
    }

    pub(super) fn search_replace_row(
        &self,
        replace: &Entity<InputState>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let found = matches!(
            &self.side_search.results,
            super::SearchResults::Found { matches, .. } if !matches.files.is_empty()
        );
        let mut replace_all = Button::new(BTN_REPLACE_ALL)
            .icon(IconName::Repeat)
            .small()
            .tooltip(TOOLTIP_REPLACE_ALL)
            .variant(ButtonVariant::Ghost);
        if found && !self.side_search.replacing {
            replace_all = replace_all.on_click(cx.listener(|this: &mut Self, _, _, cx| {
                this.ask_replace_in_all_files(cx);
            }));
        }
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_1))
            .child(field_row(replace).child(replace_all))
            .when_some(self.replace_confirm_row(cx), ParentElement::child)
            .when_some(self.undo_replace_row(cx), ParentElement::child)
            .into_any_element()
    }

    fn replace_confirm_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let plan = self.side_search.confirm_replace?;
        let pal = palette(cx);
        let confirm = Button::new(BTN_CONFIRM_REPLACE)
            .label(CONFIRM_REPLACE_LABEL)
            .small()
            .variant(ButtonVariant::Destructive)
            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                this.replace_in_all_files(cx);
            }));
        let cancel = Button::new(BTN_CANCEL_REPLACE)
            .label(CANCEL_REPLACE_LABEL)
            .small()
            .variant(ButtonVariant::Ghost)
            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                this.cancel_replace_in_all_files(cx);
            }));
        Some(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(SPACE_1))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(TEXT_XS))
                        .line_height(px(LINE_XS))
                        .text_color(rgb(pal.text_default))
                        .child(plan.question()),
                )
                .child(cancel)
                .child(confirm)
                .into_any_element(),
        )
    }

    fn undo_replace_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.side_search.last_replace.is_none() || self.side_search.replacing {
            return None;
        }
        let undo = Button::new(BTN_UNDO_REPLACE)
            .label(UNDO_REPLACE_LABEL)
            .small()
            .tooltip(TOOLTIP_UNDO_REPLACE)
            .variant(ButtonVariant::Ghost)
            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                this.undo_last_replace(cx);
            }));
        Some(
            div()
                .flex()
                .flex_row()
                .justify_end()
                .child(undo)
                .into_any_element(),
        )
    }

    fn toggle_search_option(&mut self, option: SearchOption, cx: &mut Context<Self>) {
        let mut query = self.side_search.query.clone();
        match option {
            SearchOption::Case => {
                query.case = match query.case {
                    CaseMatching::Insensitive => CaseMatching::Sensitive,
                    CaseMatching::Sensitive => CaseMatching::Insensitive,
                };
            }
            SearchOption::Word => {
                query.word = match query.word {
                    WordMatching::Anywhere => WordMatching::WholeWord,
                    WordMatching::WholeWord => WordMatching::Anywhere,
                };
            }
            SearchOption::Regex => {
                query.syntax = match query.syntax {
                    PatternSyntax::Literal => PatternSyntax::Regex,
                    PatternSyntax::Regex => PatternSyntax::Literal,
                };
            }
        }
        self.search_project_files(query, cx);
        cx.notify();
    }
}

fn field_row(field: &Entity<InputState>) -> gpui_kit::Div {
    div()
        .flex()
        .flex_row()
        .flex_none()
        .items_center()
        .gap(px(SPACE_0_5))
        .px(px(SPACE_1))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(Input::new(field).small().cleanable(true)),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_repo::{FileMatches, FoundLine};
    use std::path::PathBuf;

    #[test]
    fn the_summary_counts_results_and_files() {
        let line = FoundLine {
            line: 1,
            text: "a a".to_owned(),
            matches: vec![0..1, 2..3],
        };
        let mut matches = ProjectMatches {
            files: vec![FileMatches {
                path: PathBuf::from("a"),
                lines: vec![line],
            }],
            truncated: false,
        };
        assert_eq!(summary(&matches), "2 results in 1 file");
        matches.truncated = true;
        assert_eq!(summary(&matches), "2 results in 1 file, more not shown");
    }
}
