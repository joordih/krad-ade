mod comment_actions;
mod comment_view;
mod comments;
mod load;
mod revert;

use gpui_kit::component::diff::{
    DiffAnnotation, DiffDecoration, DiffGutterMarker, DiffHunkSeparator, DiffLineRange, DiffState,
};
use gpui_kit::component::ActiveTheme;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, Entity, FontWeight, InteractiveElement, IntoElement,
    ParentElement, SharedString, StatefulInteractiveElement, Styled,
};
use krad_domain::{DiffCommentId, FileOpening, FileView, HunkLines, NodeEdit, TerminalId};
use std::path::{Path, PathBuf};

use super::diff_view::{
    change_decorations, id_number, line_index, line_position, mode_toggle, numbered_id,
    shell_element, themed_diff, ChangeRun,
};
use super::session_changes::LoadGeneration;
use comment_actions::{CommentDraft, DraftTarget};
use comments::PlacedComment;
pub(super) use revert::reverted_block;

use super::discard::Discard;
use super::review_view::{DiffLine, LineKind};
use super::{EnvironmentShell, LiveEnv};
use crate::components::{icon, tooltip_view, Button, EmptyState, IconName};
use crate::theme::{palette, ICON_XS, LINE_XS, RADIUS_XS, SPACE_2, SPACE_3, TEXT_XS, TOOLBAR_H};

const ADDED_SIGN: &str = "+";
const REMOVED_SIGN: &str = "−";
const LOADING_TEXT: &str = "Loading…";
const NO_CHANGES_TITLE: &str = "No changes";
const NO_CHANGES_TEXT: &str = "This file matches its last commit.";
const COMMIT_SEPARATOR: &str = " · ";
const DIFF_FAILED_TITLE: &str = "Can't show the changes";
const BTN_UNFOLD_ALL: &str = "changes-tab-unfold-all";
const BTN_MODE: &str = "changes-tab-mode";
const TOOLTIP_UNFOLD_ALL: &str = "Show the whole file";
const TOOLTIP_FOLD: &str = "Fold the unchanged lines";
const FOLD_CONTEXT: usize = 3;
const BTN_REVERT_PREFIX: &str = "changes-tab-revert";
const COMMENT_PREFIX: &str = "changes-tab-comment-card";
const TOOLTIP_REVERT: &str = "Revert this block";
const MARKER_SIZE: f32 = 16.0;

struct ReadDiff {
    lines: Vec<DiffLine>,
    blocks: Vec<usize>,
    runs: Vec<ChangeRun>,
    path: SharedString,
    view: Entity<DiffState>,
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
            comments: Vec::new(),
            draft: None,
            generation: LoadGeneration::default(),
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
        let Some(group) = self.envs.get(&env).map(|live| live.selected_group) else {
            return;
        };
        let opened = NodeEdit::OpenFile {
            group,
            path: path.to_path_buf(),
            view: FileView::Changes,
            opening,
        };
        let Some(id) = self.edit_environment(&env, opened, cx).opened() else {
            return;
        };
        let Some(live) = self.envs.get_mut(&env) else {
            return;
        };
        live.focused = Some(id);
        cx.notify();
        self.save_view();
    }

    pub(super) fn shown_changes_path(&self) -> Option<PathBuf> {
        self.envs
            .get(&self.active)
            .and_then(LiveEnv::shown_diff)
            .filter(|(_, _, view)| *view == FileView::Changes)
            .map(|(_, path, _)| path)
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
        let relative = self.relative_to_root(&diff.path).display().to_string();
        let shown_path = match &diff.view {
            FileView::Commit(sha) => format!("{}{COMMIT_SEPARATOR}{relative}", sha.short()),
            FileView::Edit | FileView::Changes => relative,
        };
        let send = Self::send_review_button(live, id, cx);
        let draft = Self::draft_strip(live, id, cx);
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(Self::diff_header(
                &read.view,
                shown_path,
                (added, removed),
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
                    .child(Self::file_diff(id, diff, read, cx)),
            )
            .children(draft)
            .into_any_element()
    }

    fn diff_header(
        view: &Entity<DiffState>,
        shown_path: String,
        (added, removed): (usize, usize),
        send: Option<AnyElement>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let mono = cx.theme().mono_font_family.clone();
        let unfold_all = view.read(cx).context_lines().is_none();
        let folding = view.clone();
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
                    .font_family(mono)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(pal.text_strong))
                    .child(shown_path),
            )
            .child(
                div()
                    .flex_none()
                    .text_color(rgb(pal.diff_added))
                    .child(format!("{ADDED_SIGN}{added}")),
            )
            .child(
                div()
                    .flex_none()
                    .text_color(rgb(pal.diff_removed))
                    .child(format!("{REMOVED_SIGN}{removed}")),
            )
            .children(send)
            .child(mode_toggle(BTN_MODE, view, cx))
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
                    .on_click(move |_, _, cx| {
                        folding.update(cx, |view, cx| {
                            let context = if unfold_all { Some(FOLD_CONTEXT) } else { None };
                            view.set_context_lines(context, cx);
                        });
                    }),
            )
            .into_any_element()
    }

    fn file_diff(
        id: TerminalId,
        diff: &FileDiff,
        read: &ReadDiff,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let wash = Self::selection_wash(cx);
        let picked = diff
            .draft
            .iter()
            .flat_map(CommentDraft::picked_lines)
            .filter_map(|index| read.lines.get(index))
            .filter_map(|line| line_position(&read.path, line))
            .map(|at| {
                DiffDecoration::new(DiffLineRange::new(
                    at.path().clone(),
                    at.side(),
                    at.line(),
                    at.line(),
                ))
                .with_background(wash)
            });
        let decorations: Vec<DiffDecoration> = change_decorations(&read.path, &read.runs, pal)
            .chain(picked)
            .collect();
        let shell = cx.entity().downgrade();
        let mut view = themed_diff(&read.view)
            .header_visible(false)
            .hunk_separator(DiffHunkSeparator::LineInfo)
            .decorations(decorations)
            .annotations(diff.comments.iter().map(|comment| {
                let annotation_id = numbered_id(COMMENT_PREFIX, comment.id.0);
                comment
                    .after
                    .and_then(|line| read.lines.get(line))
                    .and_then(|line| line_position(&read.path, line))
                    .map_or_else(
                        || DiffAnnotation::file(annotation_id.clone(), read.path.clone()),
                        |at| DiffAnnotation::line(annotation_id.clone(), at),
                    )
            }))
            .render_annotation({
                let shell = shell.clone();
                move |annotation, _, cx| {
                    let comment = id_number(annotation.id());
                    shell_element(&shell, cx, |this, cx| {
                        let live = this.envs.get(&this.active)?;
                        let comment = DiffCommentId(comment?);
                        Some(Self::comment_card(live, id, comment, cx))
                    })
                }
            });
        if diff.view == FileView::Changes {
            let shell = shell.clone();
            view = view.on_add_annotation(move |range, window, cx| {
                shell
                    .update(cx, |this, cx| this.comment_on(id, range, window, cx))
                    .unwrap_or_default();
            });
        }
        if !read.blocks.is_empty() {
            let markers = read.blocks.iter().filter_map(|start| {
                let at = line_position(&read.path, read.lines.get(*start)?)?;
                Some(
                    DiffGutterMarker::new(numbered_id(BTN_REVERT_PREFIX, *start as u64), at)
                        .with_label(TOOLTIP_REVERT),
                )
            });
            view = view
                .gutter_markers(markers.collect::<Vec<_>>())
                .render_gutter_marker(move |marker, _, cx| {
                    let start =
                        id_number(marker.id()).and_then(|start| usize::try_from(start).ok());
                    shell_element(&shell, cx, |_, cx| {
                        Some(Self::revert_button(id, start?, cx))
                    })
                });
        }
        view.into_any_element()
    }

    fn comment_on(
        &mut self,
        id: TerminalId,
        range: &DiffLineRange,
        window: &mut gpui_kit::Window,
        cx: &mut Context<Self>,
    ) {
        let Some(DiffLoad::Ready(read)) = self
            .envs
            .get(&self.active)
            .and_then(|live| live.diffs.get(&id))
            .map(|diff| &diff.load)
        else {
            return;
        };
        let one = line_index(&read.lines, range.side(), range.start());
        let other = line_index(&read.lines, range.end_side(), range.end());
        let (Some(one), Some(other)) = (one, other) else {
            return;
        };
        self.start_comment(id, one, other, window, cx);
    }

    fn revert_button(id: TerminalId, start: usize, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        div()
            .id((BTN_REVERT_PREFIX, start))
            .size(px(MARKER_SIZE))
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
}

impl CommentDraft {
    fn picked_lines(&self) -> impl Iterator<Item = usize> {
        let picked = match self.target {
            DraftTarget::New { one, other, .. } => Some(HunkLines::new(one, other).range()),
            DraftTarget::Edit(_) => None,
        };
        picked.into_iter().flatten()
    }
}
