use gpui_kit::component::diff::{DiffAnnotation, DiffLinePosition, DiffState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, Entity, FontWeight, IntoElement, ParentElement, Styled,
    Window,
};
use krad_domain::{ChangeKind, LineCount, PatchBody};

use super::super::dialog_parts::{sheet_overlay, sheet_panel, sheet_title};
use super::super::diff_view::{
    change_decorations, id_number, mode_toggle, numbered_id, shell_element, themed_diff,
};
use super::super::discard::Discard;
use super::super::file_tree::{change_color, change_mark};
use super::super::EnvironmentShell;
use super::rows::{review_rows, FileRow, ReviewedFile};
use super::{close, ReviewLoad, SessionReview};
use crate::components::{Button, ButtonVariant, EmptyState, IconName};
use crate::theme::{
    palette, Palette, ICON_SM, LINE_SM, LINE_XS, RADIUS_XS, SPACE_1, SPACE_1_5, SPACE_6, TEXT_SM,
    TEXT_XS, TOOLBAR_H,
};

const SHEET_W: f32 = 960.0;
const TITLE: &str = "Session changes";
const HUNK_FOOTER: &str = "session-change-hunk";
const FILE_NOTE: &str = "session-change-note";
const BTN_MODE: &str = "session-changes-mode";
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

fn file_note(file: &ReviewedFile) -> Option<&'static str> {
    match file.patch.body {
        PatchBody::Text(_) => None,
        PatchBody::Binary => Some(NOT_TEXT),
        PatchBody::Submodule => Some(SUBMODULE_TEXT),
        PatchBody::Renamed { .. } => Some(RENAMED_TEXT),
    }
}

fn session_note(note: &'static str, cx: &Context<EnvironmentShell>) -> AnyElement {
    div()
        .text_size(px(TEXT_XS))
        .text_color(rgb(palette(cx).text_muted))
        .child(note)
        .into_any_element()
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
                    .child(Self::session_toolbar(String::new(), None, cx))
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
        let rows = review_rows(files, changes);
        let toolbar = Self::session_toolbar(summary(&rows), Some(&review.diff), cx);
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
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(toolbar)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .px(px(SPACE_6))
                    .pb(px(SPACE_6))
                    .child(Self::session_diff(review, files, cx)),
            )
            .into_any_element()
    }

    fn session_diff(
        review: &SessionReview,
        files: &[ReviewedFile],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let decorations: Vec<_> = files
            .iter()
            .flat_map(|file| change_decorations(&file.path, &file.runs, pal))
            .collect();
        let annotations: Vec<DiffAnnotation> = files
            .iter()
            .enumerate()
            .flat_map(|(index, file)| {
                let note = file_note(file).map(|_| {
                    DiffAnnotation::file(numbered_id(FILE_NOTE, index as u64), file.path.clone())
                });
                let footers = (0..file.hunks.len()).filter_map(|hunk| {
                    Some(DiffAnnotation::line(
                        numbered_id(HUNK_FOOTER, hunk as u64),
                        file.hunk_end(hunk)?,
                    ))
                });
                note.into_iter().chain(footers).collect::<Vec<_>>()
            })
            .collect();
        let shell = cx.entity().downgrade();
        let header_shell = shell.clone();
        themed_diff(&review.diff)
            .rounded(px(RADIUS_XS))
            .border_1()
            .border_color(pal.divider)
            .decorations(decorations)
            .annotations(annotations)
            .render_header(move |file, _, cx| {
                let path = file.path().clone();
                shell_element(&header_shell, cx, |this, cx| {
                    let (index, file, row) = this.session_file_at(path.as_str())?;
                    Some(Self::session_file_header(index, &row, file, cx))
                })
            })
            .render_annotation(move |annotation, _, cx| {
                let path = annotation.path().clone();
                let at = annotation.position().cloned();
                let number = id_number(annotation.id());
                shell_element(&shell, cx, |this, cx| {
                    let (index, file, row) = this.session_file_at(path.as_str())?;
                    match at {
                        Some(at) => Self::session_hunk_footer(index, file, &row, &at, cx),
                        None => number
                            .and(file_note(file))
                            .map(|note| session_note(note, cx)),
                    }
                })
            })
            .into_any_element()
    }

    fn session_file_at(&self, path: &str) -> Option<(usize, &ReviewedFile, FileRow)> {
        let review = self.session_changes.review.as_ref()?;
        let ReviewLoad::Ready { files } = &review.load else {
            return None;
        };
        let index = files.iter().position(|file| file.path.as_str() == path)?;
        let changes = self.session_changes_of(&review.env, review.session);
        let row = review_rows(files, changes).into_iter().nth(index)?;
        Some((index, files.get(index)?, row))
    }

    fn session_toolbar(
        summary: String,
        diff: Option<&Entity<DiffState>>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let mode = diff.map(|diff| mode_toggle(BTN_MODE, diff, cx));
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
            .children(mode)
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

    fn session_file_header(
        index: usize,
        row: &FileRow,
        file: &ReviewedFile,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let path = &file.patch.path;
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
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_1_5))
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
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
                            this.ask_session_revert(index, None, window, cx);
                        })),
                )
            })
            .into_any_element()
    }

    fn session_hunk_footer(
        file_index: usize,
        file: &ReviewedFile,
        row: &FileRow,
        at: &DiffLinePosition,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let hunk = file.hunk_at(at)?;
        let reviewed = row.hunks.get(hunk).copied().unwrap_or_default();
        let prints = vec![file.patch.hunks().get(hunk)?.fingerprint];
        let pal = palette(cx);
        Some(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(SPACE_1))
                .text_size(px(TEXT_XS))
                .line_height(px(LINE_XS))
                .text_color(rgb(pal.text_muted))
                .child(div().flex_1().min_w_0().child(hunk_label(file, hunk)))
                .child(
                    accept_button(reviewed, TOOLTIP_ACCEPT_HUNK).on_click(cx.listener(
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
                )
                .into_any_element(),
        )
    }

    fn ask_session_revert(
        &mut self,
        file: usize,
        hunk: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ReviewLoad::Ready { files, .. }) = self
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
        let discard = Discard::Session { patch, hunk };
        self.ask_discard(discard, window, cx);
    }
}
