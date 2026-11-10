mod comment_actions;
mod comment_view;
mod comments;
mod folds;
mod load;
mod paint;
mod revert;

use gpui_kit::component::ActiveTheme;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, uniform_list, AnyElement, Context, FontWeight, InteractiveElement, IntoElement,
    ListHorizontalSizingBehavior, ParentElement, StatefulInteractiveElement, Styled, Window,
};
use krad_domain::{FileOpening, FileView, TerminalId};
use std::collections::HashSet;
use std::ops::Range;
use std::path::{Path, PathBuf};

use super::session_changes::LoadGeneration;
use comment_actions::CommentDraft;
use comments::{with_comments, PlacedComment};
use folds::{diff_rows, widest_row, DiffRow};
pub(in crate::environment_shell) use paint::DiffColors;
pub(super) use revert::reverted_block;

use super::discard::Discard;
use super::review_view::{diff_row, DiffLine, LineKind};
use super::{EnvironmentShell, LiveEnv};
use crate::components::{icon, tooltip_view, Button, EmptyState, IconName};
use crate::theme::{
    palette, ICON_XS, LINE_XS, RADIUS_XS, SPACE_1_5, SPACE_2, SPACE_3, TEXT_XS, TOOLBAR_H,
};

const DIFF_LIST_ID: &str = "changes-tab-diff";
const DIFF_LINE_H: f32 = 20.0;
const ADDED_SIGN: &str = "+";
const REMOVED_SIGN: &str = "−";
const LOADING_TEXT: &str = "Loading…";
const NO_CHANGES_TITLE: &str = "No changes";
const NO_CHANGES_TEXT: &str = "This file matches its last commit.";
const COMMIT_SEPARATOR: &str = " · ";
const DIFF_FAILED_TITLE: &str = "Can't show the changes";
const FOLD_ROW_PREFIX: &str = "changes-tab-fold";
const BTN_UNFOLD_ALL: &str = "changes-tab-unfold-all";
const TOOLTIP_UNFOLD_ALL: &str = "Show the whole file";
const TOOLTIP_FOLD: &str = "Fold the unchanged lines";
const FOLD_INDENT: f32 = 96.0;
const REVERT_INSET: f32 = 2.0;
const BTN_REVERT_PREFIX: &str = "changes-tab-revert";
const TOOLTIP_REVERT: &str = "Revert this block";
const LINE_GROUP: &str = "changes-tab-line";

const FOLD_LINE: &str = "unchanged line";
const FOLD_LINES: &str = "unchanged lines";

fn fold_label(hidden: usize) -> String {
    let lines = if hidden == 1 { FOLD_LINE } else { FOLD_LINES };
    format!("⋯ {hidden} {lines}")
}

struct ReadDiff {
    lines: Vec<DiffLine>,
    blocks: Vec<usize>,
    rows: Vec<DiffRow>,
    widest: usize,
    colors: DiffColors,
}

enum DiffLoad {
    Pending,
    Reading,
    Ready(ReadDiff),
    Failed(String),
}

pub(super) struct FileDiff {
    path: PathBuf,
    view: FileView,
    load: DiffLoad,
    unfolded: HashSet<usize>,
    unfold_all: bool,
    comments: Vec<PlacedComment>,
    draft: Option<CommentDraft>,
    generation: LoadGeneration,
}

impl FileDiff {
    fn new(path: PathBuf, view: FileView) -> Self {
        Self {
            path,
            view,
            load: DiffLoad::Pending,
            unfolded: HashSet::new(),
            unfold_all: false,
            comments: Vec::new(),
            draft: None,
            generation: LoadGeneration::default(),
        }
    }

    fn refold(&mut self) {
        if let DiffLoad::Ready(read) = &mut self.load {
            read.rows = with_comments(
                diff_rows(&read.lines, &self.unfolded, self.unfold_all),
                &self.comments,
            );
            read.widest = widest_row(&read.rows, &read.lines);
        }
    }
}

impl LiveEnv {
    pub(super) fn diff_path(&self, tab: TerminalId) -> Option<&Path> {
        self.diffs.get(&tab).map(|diff| diff.path.as_path())
    }

    pub(super) fn diff_lines(&self, tab: TerminalId) -> Option<(PathBuf, Vec<DiffLine>)> {
        let diff = self.diffs.get(&tab)?;
        match &diff.load {
            DiffLoad::Ready(read) => Some((diff.path.clone(), read.lines.clone())),
            _ => None,
        }
    }

    fn shown_diff(&self) -> Option<(TerminalId, PathBuf, FileView)> {
        let id = self.shown_tab()?;
        let file = self
            .data
            .open_file_of(id)
            .filter(|file| file.view != FileView::Edit)?;
        Some((id, file.path.clone(), file.view.clone()))
    }
}

fn count_lines(lines: &[DiffLine], kind: LineKind) -> usize {
    lines.iter().filter(|line| line.kind == kind).count()
}

impl EnvironmentShell {
    pub(super) fn open_changes_tab(
        &mut self,
        path: &Path,
        opening: FileOpening,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        let Some(live) = self.envs.get_mut(&env) else {
            return;
        };
        let id = live
            .data
            .open_file_as(live.selected_group, path, FileView::Changes, opening);
        live.focused = Some(id);
        cx.notify();
        self.store();
    }

    pub(super) fn shown_changes_path(&self) -> Option<PathBuf> {
        self.envs
            .get(&self.active)
            .and_then(LiveEnv::shown_diff)
            .filter(|(_, _, view)| *view == FileView::Changes)
            .map(|(_, path, _)| path)
    }

    fn toggle_fold(&mut self, id: TerminalId, first: Option<usize>, cx: &mut Context<Self>) {
        let env = self.active.clone();
        let Some(diff) = self
            .envs
            .get_mut(&env)
            .and_then(|live| live.diffs.get_mut(&id))
        else {
            return;
        };
        if let Some(first) = first {
            diff.unfolded.insert(first);
        } else {
            diff.unfold_all = !diff.unfold_all;
            diff.unfolded.clear();
        }
        diff.refold();
        cx.notify();
    }

    pub(super) fn changes_tab_body(
        &self,
        live: &LiveEnv,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(diff) = live.diffs.get(&id) else {
            return div().flex_1().into_any_element();
        };
        let read = match &diff.load {
            DiffLoad::Pending | DiffLoad::Reading => {
                return EmptyState::new(IconName::Loading, LOADING_TEXT, String::new())
                    .into_any_element();
            }
            DiffLoad::Failed(reason) => {
                return EmptyState::new(IconName::File, DIFF_FAILED_TITLE, reason.clone())
                    .into_any_element();
            }
            DiffLoad::Ready(read) => read,
        };
        let added = count_lines(&read.lines, LineKind::Added);
        let removed = count_lines(&read.lines, LineKind::Removed);
        if added + removed == 0 {
            return EmptyState::new(IconName::CheckCircle, NO_CHANGES_TITLE, NO_CHANGES_TEXT)
                .into_any_element();
        }
        let mono = cx.theme().mono_font_family.clone();
        let relative = self.relative_to_root(&diff.path).display().to_string();
        let shown_path = match &diff.view {
            FileView::Commit(sha) => format!("{}{COMMIT_SEPARATOR}{relative}", sha.short()),
            FileView::Edit | FileView::Changes => relative,
        };
        let list = Self::diff_list(id, read, cx);
        let unfold_all = diff.unfold_all;
        let send = Self::send_review_button(live, id, cx);
        let draft = Self::draft_strip(live, id, cx);
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(Self::diff_header(
                id,
                shown_path,
                (added, removed),
                unfold_all,
                send,
                cx,
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .font_family(mono)
                    .text_size(px(TEXT_XS))
                    .line_height(px(DIFF_LINE_H))
                    .child(list),
            )
            .children(draft)
            .into_any_element()
    }

    fn diff_header(
        id: TerminalId,
        shown_path: String,
        (added, removed): (usize, usize),
        unfold_all: bool,
        send: Option<AnyElement>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let mono = cx.theme().mono_font_family.clone();
        div()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SPACE_2))
            .h(px(TOOLBAR_H))
            .px(px(SPACE_3))
            .border_b_1()
            .border_color(pal.divider)
            .text_size(px(TEXT_XS))
            .line_height(px(LINE_XS))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(mono.clone())
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(pal.text_strong))
                    .child(shown_path),
            )
            .child(
                div()
                    .flex_none()
                    .text_color(rgb(pal.success_fg))
                    .child(format!("{ADDED_SIGN}{added}")),
            )
            .child(
                div()
                    .flex_none()
                    .text_color(rgb(pal.error_fg))
                    .child(format!("{REMOVED_SIGN}{removed}")),
            )
            .children(send)
            .child(
                Button::new(BTN_UNFOLD_ALL)
                    .icon(if unfold_all {
                        IconName::CollapseAll
                    } else {
                        IconName::Expand
                    })
                    .small()
                    .tooltip(if unfold_all {
                        TOOLTIP_FOLD
                    } else {
                        TOOLTIP_UNFOLD_ALL
                    })
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.toggle_fold(id, None, cx);
                    })),
            )
            .into_any_element()
    }

    fn diff_list(id: TerminalId, read: &ReadDiff, cx: &mut Context<Self>) -> AnyElement {
        uniform_list(
            (DIFF_LIST_ID, id.as_u64()),
            read.rows.len(),
            cx.processor(
                move |this: &mut Self, range: Range<usize>, _: &mut Window, cx| {
                    let pal = palette(cx);
                    let wash = Self::selection_wash(cx);
                    let Some(live) = this.envs.get(&this.active) else {
                        return Vec::new();
                    };
                    let Some(diff) = live.diffs.get(&id) else {
                        return Vec::new();
                    };
                    let DiffLoad::Ready(read) = &diff.load else {
                        return Vec::new();
                    };
                    let commentable = diff.view == FileView::Changes;
                    read.rows
                        .get(range)
                        .unwrap_or_default()
                        .iter()
                        .map(|row| match row {
                            DiffRow::Line(index) => read.lines.get(*index).map_or_else(
                                || div().into_any_element(),
                                |line| {
                                    let starts_block = read.blocks.contains(index);
                                    let picked = diff
                                        .draft
                                        .as_ref()
                                        .is_some_and(|draft| draft.covers(*index));
                                    div()
                                        .relative()
                                        .group(LINE_GROUP)
                                        .h(px(DIFF_LINE_H))
                                        .child(diff_row(line, read.colors.line(*index), pal))
                                        .when(picked, |row| row.bg(wash))
                                        .when(starts_block, |row| {
                                            row.child(Self::revert_button(id, *index, cx))
                                        })
                                        .when(commentable && line.kind != LineKind::Hunk, |row| {
                                            row.child(Self::comment_button(
                                                id, *index, LINE_GROUP, cx,
                                            ))
                                        })
                                        .into_any_element()
                                },
                            ),
                            DiffRow::Fold { first, hidden } => {
                                Self::fold_row(id, *first, *hidden, cx)
                            }
                            DiffRow::Comment { id: comment, part } => {
                                Self::comment_row(live, id, *comment, *part, cx)
                            }
                        })
                        .collect::<Vec<_>>()
                },
            ),
        )
        .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
        .with_width_from_item(Some(read.widest))
        .flex_1()
        .min_h_0()
        .into_any_element()
    }

    fn revert_button(id: TerminalId, start: usize, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        div()
            .id((BTN_REVERT_PREFIX, start))
            .absolute()
            .left(px(REVERT_INSET))
            .top(px(REVERT_INSET))
            .size(px(DIFF_LINE_H - 2.0 * REVERT_INSET))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(RADIUS_XS))
            .bg(rgb(pal.bg_panel))
            .cursor_pointer()
            .hover(|style| style.bg(pal.bg_selected))
            .tooltip(|_, cx| tooltip_view(TOOLTIP_REVERT.into(), cx))
            .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                this.ask_discard(Discard::Block { tab: id, start }, window, cx);
            }))
            .child(icon(IconName::Undo, ICON_XS, pal.text_muted))
            .into_any_element()
    }

    fn fold_row(id: TerminalId, first: usize, hidden: usize, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        div()
            .id((FOLD_ROW_PREFIX, first))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_1_5))
            .h(px(DIFF_LINE_H))
            .pl(px(FOLD_INDENT))
            .bg(pal.bg_hover)
            .text_color(rgb(pal.text_muted))
            .cursor_pointer()
            .hover(|style| style.bg(pal.bg_selected))
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.toggle_fold(id, Some(first), cx);
            }))
            .child(icon(IconName::ChevronDown, ICON_XS, pal.text_muted))
            .child(fold_label(hidden))
            .into_any_element()
    }
}
