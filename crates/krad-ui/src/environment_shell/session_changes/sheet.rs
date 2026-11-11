use gpui_kit::component::ActiveTheme;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled, Window,
};
use krad_repo::{ChangeKind, LineCount, PatchBody};

use super::super::dialog_parts::{sheet_overlay, sheet_panel, sheet_title};
use super::super::discard::Discard;
use super::super::file_tree::{change_color, change_mark};
use super::super::review_view::{diff_row, DiffLine};
use super::super::EnvironmentShell;
use super::rows::{review_rows, FileRow, HunkRow, ReviewedFile};
use super::{close, ReviewLoad, SessionReview};
use crate::components::{icon, Button, ButtonVariant, EmptyState, IconName};
use crate::theme::{
    palette, Palette, ICON_SM, LINE_SM, LINE_XS, RADIUS_XS, SPACE_1, SPACE_1_5, SPACE_2, SPACE_3,
    SPACE_6, TEXT_SM, TEXT_XS, TOOLBAR_H,
};

const SHEET_W: f32 = 960.0;
const DIFF_LINE_H: f32 = 20.0;
const FILE_ROW_H: f32 = 32.0;
const TITLE: &str = "Session changes";
const BODY_ID: &str = "session-changes-body";
const FILE_BLOCK: &str = "session-change-file";
const HUNK_BLOCK: &str = "session-change-hunk";
const FILE_HEADER: &str = "session-change-file-header";
const BTN_CLOSE: &str = "session-changes-close";
const BTN_REFRESH: &str = "session-changes-refresh";
const BTN_RESTART: &str = "session-changes-restart";
const BTN_ACCEPT: &str = "session-change-accept";
const BTN_REVERT: &str = "session-change-revert";
const TOOLTIP_REFRESH: &str = "Refresh";
const TOOLTIP_RESTART: &str = "Start over from the files as they are now";
const ACCEPT_LABEL: &str = "Accept";
const ACCEPTED_LABEL: &str = "Accepted";
const TOOLTIP_ACCEPT_FILE: &str = "Mark every change in this file as reviewed";
const TOOLTIP_ACCEPT_HUNK: &str = "Mark this block as reviewed";
const TOOLTIP_UNACCEPT: &str = "Mark as not reviewed";
const TOOLTIP_REVERT_FILE: &str = "Revert this file to how it was when the session started";
const TOOLTIP_REVERT_HUNK: &str = "Revert this block to how it was when the session started";
const LOADING_TEXT: &str = "Loading…";
const NO_REPO_TITLE: &str = "No repository";
const NO_REPO_TEXT: &str = "This session's group is not in a git repository.";
const FAILED_TITLE: &str = "Can't show the session's changes";
const CLEAN_TITLE: &str = "No changes in this session";
const CLEAN_TEXT: &str = "Files changed after the session started appear here.";
const NOT_TEXT: &str = "This file is not text, so only the whole file can be reverted.";
const SUBMODULE_TEXT: &str = "This is a submodule; review it in its own repository.";
const RENAMED_TEXT: &str = "This file was renamed; review and revert it with git.";
const LINES_LABEL: &str = "Lines";
const ADDED_SIGN: &str = "+";
const REMOVED_SIGN: &str = "−";
const FILES_ONE: &str = "file";
const FILES_MANY: &str = "files";
const REVIEWED_SUFFIX: &str = "reviewed";
const SUMMARY_SEPARATOR: &str = " · ";

fn line_counts(count: LineCount, pal: &Palette) -> impl Iterator<Item = AnyElement> {
    [
        (count.added, ADDED_SIGN, pal.diff_added),
        (count.removed, REMOVED_SIGN, pal.diff_removed),
    ]
    .into_iter()
    .filter(|(lines, _, _)| *lines > 0)
    .map(|(lines, sign, color)| {
        div()
            .flex_none()
            .text_size(px(TEXT_XS))
            .text_color(rgb(color))
            .child(format!("{sign}{lines}"))
            .into_any_element()
    })
}

fn summary(rows: &[FileRow]) -> String {
    let files = if rows.len() == 1 {
        FILES_ONE
    } else {
        FILES_MANY
    };
    let reviewed = rows.iter().filter(|row| row.reviewed).count();
    format!(
        "{} {files}{SUMMARY_SEPARATOR}{reviewed} {REVIEWED_SUFFIX}",
        rows.len()
    )
}

fn hunk_label(file: &ReviewedFile, hunk: usize) -> String {
    let start = file
        .patch
        .hunks()
        .get(hunk)
        .map_or(0, |hunk| hunk.new_start);
    format!("{LINES_LABEL} {start}")
}

fn accept_button(reviewed: bool, tooltip: &'static str) -> Button {
    Button::new(BTN_ACCEPT)
        .icon(if reviewed {
            IconName::CheckCircle
        } else {
            IconName::Tick
        })
        .label(if reviewed {
            ACCEPTED_LABEL
        } else {
            ACCEPT_LABEL
        })
        .small()
        .variant(if reviewed {
            ButtonVariant::Secondary
        } else {
            ButtonVariant::Ghost
        })
        .tooltip(if reviewed { TOOLTIP_UNACCEPT } else { tooltip })
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn session_changes_sheet(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let review = self.session_changes.review.as_ref()?;
        let pal = palette(cx);
        let subtitle = self
            .envs
            .get(&review.env)
            .and_then(|live| live.data.session(review.session))
            .map(|session| session.name.clone());
        let body = self.session_changes_body(review, cx);
        let sheet = sheet_panel(SHEET_W, pal)
            .h_full()
            .child(sheet_title(TITLE, subtitle, BTN_CLOSE, close, cx))
            .child(body);
        Some(sheet_overlay(sheet, close, cx))
    }

    fn session_changes_body(&self, review: &SessionReview, cx: &mut Context<Self>) -> AnyElement {
        let files = match &review.load {
            ReviewLoad::Reading => {
                return EmptyState::new(IconName::Loading, LOADING_TEXT, String::new())
                    .into_any_element();
            }
            ReviewLoad::NoRepository => {
                return EmptyState::new(IconName::GitBranch, NO_REPO_TITLE, NO_REPO_TEXT)
                    .into_any_element();
            }
            ReviewLoad::Failed(reason) => {
                return div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .child(Self::session_toolbar(String::new(), cx))
                    .child(EmptyState::new(
                        IconName::CancelCircle,
                        FAILED_TITLE,
                        reason.clone(),
                    ))
                    .into_any_element();
            }
            ReviewLoad::Ready { files, .. } => files,
        };
        let changes = self.session_changes_of(&review.env, review.session);
        let rows = review_rows(files, changes, &review.expanded);
        let toolbar = Self::session_toolbar(summary(&rows), cx);
        if rows.is_empty() {
            return div()
                .flex()
                .flex_col()
                .flex_1()
                .child(toolbar)
                .child(EmptyState::new(
                    IconName::CheckCircle,
                    CLEAN_TITLE,
                    CLEAN_TEXT,
                ))
                .into_any_element();
        }
        let mono = cx.theme().mono_font_family.clone();
        let blocks: Vec<AnyElement> = rows
            .iter()
            .filter_map(|row| {
                let file = files.get(row.file)?;
                Some(Self::session_file_block(row, file, mono.clone(), cx))
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(toolbar)
            .child(
                div()
                    .id(BODY_ID)
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .gap(px(SPACE_2))
                    .px(px(SPACE_6))
                    .pb(px(SPACE_6))
                    .children(blocks),
            )
            .into_any_element()
    }

    fn session_toolbar(summary: String, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        div()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SPACE_1))
            .h(px(TOOLBAR_H))
            .px(px(SPACE_6))
            .text_size(px(TEXT_XS))
            .line_height(px(LINE_XS))
            .text_color(rgb(pal.text_muted))
            .child(div().flex_1().min_w_0().truncate().child(summary))
            .child(
                Button::new(BTN_RESTART)
                    .icon(IconName::Repeat)
                    .small()
                    .tooltip(TOOLTIP_RESTART)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.restart_session_changes(cx);
                    })),
            )
            .child(
                Button::new(BTN_REFRESH)
                    .icon(IconName::Refresh)
                    .small()
                    .tooltip(TOOLTIP_REFRESH)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.reload_session_changes(cx);
                    })),
            )
            .into_any_element()
    }

    fn session_file_block(
        row: &FileRow,
        file: &ReviewedFile,
        mono: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let index = row.file;
        let note = match file.patch.body {
            PatchBody::Text(_) => None,
            PatchBody::Binary => Some(NOT_TEXT),
            PatchBody::Submodule => Some(SUBMODULE_TEXT),
            PatchBody::Renamed { .. } => Some(RENAMED_TEXT),
        };
        let hunks: Vec<AnyElement> = row
            .hunks
            .iter()
            .filter_map(|hunk| {
                let lines = file.hunks.get(hunk.hunk)?;
                Some(Self::session_hunk_block(index, *hunk, file, lines, cx))
            })
            .collect();
        div()
            .id((FILE_BLOCK, index))
            .flex()
            .flex_col()
            .flex_none()
            .rounded(px(RADIUS_XS))
            .border_1()
            .border_color(pal.divider)
            .overflow_hidden()
            .child(Self::session_file_header(row, file, cx))
            .when(row.expanded, |block| {
                block
                    .children(note.map(|note| {
                        div()
                            .px(px(SPACE_3))
                            .py(px(SPACE_2))
                            .text_size(px(TEXT_XS))
                            .text_color(rgb(pal.text_muted))
                            .child(note)
                    }))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .font_family(mono)
                            .text_size(px(TEXT_XS))
                            .line_height(px(DIFF_LINE_H))
                            .children(hunks),
                    )
            })
            .into_any_element()
    }

    fn session_file_header(
        row: &FileRow,
        file: &ReviewedFile,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let index = row.file;
        let path = file.patch.path.clone();
        let kind = ChangeKind::from(file.patch.status);
        let color = change_color(kind, pal);
        let name = path
            .file_name()
            .map_or_default(|name| name.to_string_lossy().into_owned());
        let folder = path
            .parent()
            .map_or_else(String::new, |folder| folder.display().to_string());
        let prints = file.prints.clone();
        let reverts = !matches!(
            file.patch.body,
            PatchBody::Submodule | PatchBody::Renamed { .. }
        );
        div()
            .id(FILE_HEADER)
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_1_5))
            .h(px(FILE_ROW_H))
            .px(px(SPACE_2))
            .bg(rgb(pal.bg_panel))
            .cursor_pointer()
            .hover(|style| style.bg(pal.bg_hover))
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.toggle_session_file(&path, cx);
            }))
            .child(icon(
                if row.expanded {
                    IconName::ChevronDown
                } else {
                    IconName::ChevronRight
                },
                ICON_SM,
                pal.text_subtle,
            ))
            .child(
                div()
                    .flex_none()
                    .w(px(ICON_SM))
                    .text_size(px(TEXT_XS))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(color))
                    .child(change_mark(kind)),
            )
            .child(
                div()
                    .flex_none()
                    .max_w_full()
                    .truncate()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(if row.reviewed {
                        pal.text_muted
                    } else {
                        pal.text_strong
                    }))
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
            .children(line_counts(file.count, pal))
            .child(
                accept_button(row.reviewed, TOOLTIP_ACCEPT_FILE).on_click(cx.listener(
                    move |this: &mut Self, _, _, cx| {
                        cx.stop_propagation();
                        this.toggle_session_reviewed(&prints, cx);
                    },
                )),
            )
            .when(reverts, |header| {
                header.child(
                    Button::new(BTN_REVERT)
                        .icon(IconName::Undo)
                        .small()
                        .tooltip(TOOLTIP_REVERT_FILE)
                        .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                            cx.stop_propagation();
                            this.ask_session_revert(index, None, window, cx);
                        })),
                )
            })
            .into_any_element()
    }

    fn session_hunk_block(
        file_index: usize,
        row: HunkRow,
        file: &ReviewedFile,
        lines: &[DiffLine],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let prints = file
            .patch
            .hunks()
            .get(row.hunk)
            .map_or_else(Vec::new, |hunk| vec![hunk.fingerprint]);
        let hunk = row.hunk;
        div()
            .id((HUNK_BLOCK, hunk))
            .flex()
            .flex_col()
            .border_t_1()
            .border_color(pal.divider)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(SPACE_1))
                    .h(px(FILE_ROW_H))
                    .px(px(SPACE_2))
                    .bg(rgb(pal.bg_panel))
                    .text_color(rgb(pal.text_muted))
                    .child(div().flex_1().min_w_0().child(hunk_label(file, hunk)))
                    .child(
                        accept_button(row.reviewed, TOOLTIP_ACCEPT_HUNK).on_click(cx.listener(
                            move |this: &mut Self, _, _, cx| {
                                this.toggle_session_reviewed(&prints, cx);
                            },
                        )),
                    )
                    .child(
                        Button::new(BTN_REVERT)
                            .icon(IconName::Undo)
                            .small()
                            .tooltip(TOOLTIP_REVERT_HUNK)
                            .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                                this.ask_session_revert(file_index, Some(hunk), window, cx);
                            })),
                    ),
            )
            .children(
                lines
                    .iter()
                    .map(|line| div().h(px(DIFF_LINE_H)).child(diff_row(line, None, pal))),
            )
            .into_any_element()
    }

    fn ask_session_revert(
        &mut self,
        file: usize,
        hunk: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ReviewLoad::Ready { repo, files }) = self
            .session_changes
            .review
            .as_ref()
            .map(|review| &review.load)
        else {
            return;
        };
        let Some(patch) = files.get(file).map(|file| file.patch.clone()) else {
            return;
        };
        let discard = Discard::Session {
            repo: repo.clone(),
            patch,
            hunk,
        };
        self.ask_discard(discard, window, cx);
    }
}
