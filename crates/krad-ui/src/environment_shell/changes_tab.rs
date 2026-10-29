mod folds;

use gpui_kit::component::ActiveTheme;
use gpui_kit::{
    div, px, rgb, uniform_list, AnyElement, AppContext as _, Context, FontWeight,
    InteractiveElement, IntoElement, ListHorizontalSizingBehavior, ParentElement,
    StatefulInteractiveElement, Styled, Window,
};
use krad_domain::{EnvironmentName, FileOpening, FileView, TerminalId};
use krad_repo::{working_diff, FileChange};
use std::collections::HashSet;
use std::ops::Range;
use std::path::{Path, PathBuf};

use folds::{diff_rows, widest_row, DiffRow};

use super::review_view::{diff_row, numbered_lines, DiffLine, LineKind};
use super::{EnvironmentShell, LiveEnv};
use crate::components::{icon, Button, EmptyState, IconName};
use crate::theme::{palette, ICON_XS, LINE_XS, SPACE_1_5, SPACE_2, SPACE_3, TEXT_XS, TOOLBAR_H};

const DIFF_LIST_ID: &str = "changes-tab-diff";
const DIFF_LINE_H: f32 = 20.0;
const ADDED_SIGN: &str = "+";
const REMOVED_SIGN: &str = "−";
const LOADING_TEXT: &str = "Loading…";
const NO_CHANGES_TITLE: &str = "No changes";
const NO_CHANGES_TEXT: &str = "This file matches its last commit.";
const DIFF_FAILED_TITLE: &str = "Can't show the changes";
const FOLD_ROW_PREFIX: &str = "changes-tab-fold";
const BTN_UNFOLD_ALL: &str = "changes-tab-unfold-all";
const TOOLTIP_UNFOLD_ALL: &str = "Show the whole file";
const TOOLTIP_FOLD: &str = "Fold the unchanged lines";
const FOLD_INDENT: f32 = 96.0;

const FOLD_LINE: &str = "unchanged line";
const FOLD_LINES: &str = "unchanged lines";

fn fold_label(hidden: usize) -> String {
    let lines = if hidden == 1 { FOLD_LINE } else { FOLD_LINES };
    format!("⋯ {hidden} {lines}")
}

struct ReadDiff {
    lines: Vec<DiffLine>,
    rows: Vec<DiffRow>,
    widest: usize,
}

enum DiffLoad {
    Pending,
    Reading,
    Ready(ReadDiff),
    Failed(String),
}

pub(super) struct FileDiff {
    path: PathBuf,
    load: DiffLoad,
    unfolded: HashSet<usize>,
    unfold_all: bool,
}

impl FileDiff {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            load: DiffLoad::Pending,
            unfolded: HashSet::new(),
            unfold_all: false,
        }
    }

    fn refold(&mut self) {
        if let DiffLoad::Ready(read) = &mut self.load {
            read.rows = diff_rows(&read.lines, &self.unfolded, self.unfold_all);
            read.widest = widest_row(&read.rows, &read.lines);
        }
    }
}

impl LiveEnv {
    fn shown_changes(&self) -> Option<(TerminalId, PathBuf)> {
        let id = self.shown_tab()?;
        let file = self
            .data
            .open_file_of(id)
            .filter(|file| file.view == FileView::Changes)?;
        Some((id, file.path.clone()))
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
            .and_then(LiveEnv::shown_changes)
            .map(|(_, path)| path)
    }

    pub(super) fn sync_changes_tabs(&mut self, cx: &mut Context<Self>) {
        let env = self.active.clone();
        let Some(live) = self.envs.get_mut(&env) else {
            return;
        };
        let open: Vec<TerminalId> = live
            .data
            .nodes
            .iter()
            .filter(|node| node.file.is_some())
            .map(|node| node.id)
            .collect();
        live.diffs.retain(|id, _| open.contains(id));
        let Some((id, path)) = live.shown_changes() else {
            return;
        };
        let diff = live
            .diffs
            .entry(id)
            .or_insert_with(|| FileDiff::new(path.clone()));
        if diff.path != path {
            *diff = FileDiff::new(path.clone());
        }
        if !matches!(diff.load, DiffLoad::Pending) {
            return;
        }
        diff.load = DiffLoad::Reading;
        let change = self.change_at(&path);
        Self::read_diff(env, id, change, cx);
    }

    pub(super) fn forget_diffs(&mut self) {
        for live in self.envs.values_mut() {
            for diff in live.diffs.values_mut() {
                diff.load = DiffLoad::Pending;
            }
        }
    }

    fn change_at(&self, path: &Path) -> Option<(PathBuf, FileChange)> {
        self.git.values().find_map(|git| {
            git.status
                .changes
                .iter()
                .find(|change| git.repo_root.join(&change.path) == path)
                .map(|change| (git.repo_root.clone(), change.clone()))
        })
    }

    fn read_diff(
        env: EnvironmentName,
        id: TerminalId,
        change: Option<(PathBuf, FileChange)>,
        cx: &mut Context<Self>,
    ) {
        let reading = cx.background_spawn(async move {
            change.map_or_else(
                || Ok(Vec::new()),
                |(repo, change)| working_diff(&repo, &change).map(|diff| numbered_lines(&diff)),
            )
        });
        cx.spawn(async move |this, cx| {
            let read = reading.await;
            this.update(cx, |shell, cx| {
                if let Some(diff) = shell
                    .envs
                    .get_mut(&env)
                    .and_then(|live| live.diffs.get_mut(&id))
                {
                    diff.load = match read {
                        Ok(lines) => DiffLoad::Ready(ReadDiff {
                            lines,
                            rows: Vec::new(),
                            widest: 0,
                        }),
                        Err(error) => DiffLoad::Failed(error.to_string()),
                    };
                    diff.refold();
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
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
        let shown_path = self.relative_to_root(&diff.path).display().to_string();
        let list = Self::diff_list(id, read, cx);
        let unfold_all = diff.unfold_all;
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
            .into_any_element()
    }

    fn diff_header(
        id: TerminalId,
        shown_path: String,
        (added, removed): (usize, usize),
        unfold_all: bool,
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
                    let Some(DiffLoad::Ready(read)) = this
                        .envs
                        .get(&this.active)
                        .and_then(|live| live.diffs.get(&id))
                        .map(|diff| &diff.load)
                    else {
                        return Vec::new();
                    };
                    read.rows
                        .get(range)
                        .unwrap_or_default()
                        .iter()
                        .map(|row| match row {
                            DiffRow::Line(index) => read.lines.get(*index).map_or_else(
                                || div().into_any_element(),
                                |line| {
                                    div()
                                        .h(px(DIFF_LINE_H))
                                        .child(diff_row(line, pal))
                                        .into_any_element()
                                },
                            ),
                            DiffRow::Fold { first, hidden } => {
                                Self::fold_row(id, *first, *hidden, cx)
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
