use gpui_kit::component::InteractiveElementExt as _;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, FontWeight, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, ParentElement, StatefulInteractiveElement, Styled,
};
use krad_domain::{CommitLabel, FileOpening, GraphRow, HistoryCommit};
use krad_repo::CommitFile;
use std::path::Path;
use std::time::SystemTime;

use super::super::super::super::context_menu::{MenuAction, MenuEntry};
use super::super::super::super::file_tree::{change_color, change_mark};
use super::super::super::super::surface::age_label;
use super::super::super::super::EnvironmentShell;
use super::super::ChangesAction;
use super::graph_cell::{graph_cell, GraphCell, LANE_W};
use super::FilesLoad;
use crate::components::{tooltip_view, IconName};
use crate::theme::{
    palette, Palette, ICON_SM, LINE_SM, RADIUS_XS, SPACE_0_5, SPACE_1, SPACE_1_5, SPACE_2,
    TEXT_2XS, TEXT_SM, TEXT_XS,
};

const ROW_H: f32 = 24.0;
const COMMIT_ROW_PREFIX: &str = "history-commit";
const FILE_ROW_PREFIX: &str = "history-file";
const NOTE_ROW_PREFIX: &str = "history-note";
const SHOWN_LABELS: usize = 2;
const COPY_HASH_LABEL: &str = "Copy commit hash";
const COPY_MESSAGE_LABEL: &str = "Copy commit message";
const LOADING_NOTE: &str = "Loading…";
const EMPTY_NOTE: &str = "No files changed";
const DETAIL_SEPARATOR: &str = " · ";
const ADDED_SIGN: &str = "+";
const REMOVED_SIGN: &str = "−";

fn label_text(label: &CommitLabel) -> &str {
    match label {
        CommitLabel::Branch { name, .. } | CommitLabel::Remote(name) | CommitLabel::Tag(name) => {
            name
        }
    }
}

fn label_color(label: &CommitLabel, pal: &Palette) -> u32 {
    match label {
        CommitLabel::Branch { current: true, .. } => pal.accent,
        CommitLabel::Branch { .. } => pal.text_default,
        CommitLabel::Remote(_) => pal.info_fg,
        CommitLabel::Tag(_) => pal.warning_fg,
    }
}

fn label_chip(label: &CommitLabel, pal: &Palette) -> AnyElement {
    let color = label_color(label, pal);
    div()
        .flex_none()
        .max_w(px(LANE_W * 8.0))
        .truncate()
        .px(px(SPACE_1))
        .rounded(px(RADIUS_XS))
        .border_1()
        .border_color(rgb(color))
        .text_size(px(TEXT_2XS))
        .text_color(rgb(color))
        .child(label_text(label).to_owned())
        .into_any_element()
}

fn commit_age(commit: &HistoryCommit) -> String {
    SystemTime::now()
        .duration_since(commit.time)
        .map_or_else(|_| String::new(), age_label)
}

fn commit_details(commit: &HistoryCommit) -> String {
    let labels: Vec<&str> = commit.labels.iter().map(label_text).collect();
    let mut head = vec![
        commit.sha.short().to_owned(),
        commit.author.clone(),
        commit_age(commit),
    ];
    if !labels.is_empty() {
        head.push(labels.join(", "));
    }
    format!("{}\n{}", head.join(DETAIL_SEPARATOR), commit.subject)
}

fn commit_menu(commit: &HistoryCommit) -> Vec<MenuEntry> {
    vec![
        MenuEntry::item(
            COPY_HASH_LABEL,
            Some(IconName::Copy),
            MenuAction::Changes(ChangesAction::Copy(commit.sha.0.clone())),
        ),
        MenuEntry::item(
            COPY_MESSAGE_LABEL,
            Some(IconName::Copy),
            MenuAction::Changes(ChangesAction::Copy(commit.subject.clone())),
        ),
    ]
}

fn row_base(pal: &Palette) -> gpui_kit::Div {
    div()
        .flex()
        .flex_row()
        .w_full()
        .items_center()
        .gap(px(SPACE_1_5))
        .h(px(ROW_H))
        .pr(px(SPACE_2))
        .rounded(px(RADIUS_XS))
        .text_size(px(TEXT_SM))
        .line_height(px(LINE_SM))
        .text_color(rgb(pal.text_default))
}

impl EnvironmentShell {
    pub(super) fn commit_row(
        index: usize,
        commit: &HistoryCommit,
        graph: &GraphRow,
        (lanes, expanded): (usize, bool),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let sha = commit.sha.clone();
        let menu = commit_menu(commit);
        let details = commit_details(commit);
        row_base(pal)
            .id((COMMIT_ROW_PREFIX, index))
            .cursor_pointer()
            .when(expanded, |row| row.bg(pal.bg_selected))
            .hover(|style| style.bg(pal.bg_hover))
            .tooltip(move |_, cx| tooltip_view(details.clone().into(), cx))
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| this.toggle_commit(&sha, cx)))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this: &mut Self, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.open_menu_at(event, menu.clone(), cx);
                }),
            )
            .child(graph_cell(GraphCell::Node(graph.clone()), lanes))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(pal.text_strong))
                    .child(commit.subject.clone()),
            )
            .children(
                commit
                    .labels
                    .iter()
                    .take(SHOWN_LABELS)
                    .map(|label| label_chip(label, pal)),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(TEXT_XS))
                    .text_color(rgb(pal.text_muted))
                    .child(commit_age(commit)),
            )
            .into_any_element()
    }

    pub(super) fn commit_file_row(
        row: usize,
        commit: &HistoryCommit,
        file: &CommitFile,
        (graph, lanes): (&GraphRow, usize),
        repo: &Path,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let change = &file.change;
        let color = change_color(change.kind, pal);
        let name = change
            .path
            .file_name()
            .map_or_default(|name| name.to_string_lossy().into_owned());
        let folder = change
            .path
            .parent()
            .map(|parent| parent.display().to_string())
            .filter(|folder| !folder.is_empty());
        let path = repo.join(&change.path);
        let pinned = path.clone();
        let (sha, pinned_sha) = (commit.sha.clone(), commit.sha.clone());
        row_base(pal)
            .id((FILE_ROW_PREFIX, row))
            .cursor_pointer()
            .hover(|style| style.bg(pal.bg_hover))
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.open_commit_file(&path, sha.clone(), FileOpening::Preview, cx);
            }))
            .on_double_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.open_commit_file(&pinned, pinned_sha.clone(), FileOpening::Pinned, cx);
            }))
            .child(graph_cell(GraphCell::below(graph), lanes))
            .child(
                div()
                    .flex_none()
                    .w(px(ICON_SM))
                    .text_size(px(TEXT_XS))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(color))
                    .child(change_mark(change.kind)),
            )
            .child(
                div()
                    .flex_none()
                    .max_w_full()
                    .truncate()
                    .text_color(rgb(color))
                    .child(name),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(TEXT_XS))
                    .text_color(rgb(pal.text_muted))
                    .children(folder),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_none()
                    .gap(px(SPACE_0_5))
                    .text_size(px(TEXT_XS))
                    .child(
                        div()
                            .text_color(rgb(pal.success_fg))
                            .child(format!("{ADDED_SIGN}{}", file.lines.added)),
                    )
                    .child(
                        div()
                            .text_color(rgb(pal.error_fg))
                            .child(format!("{REMOVED_SIGN}{}", file.lines.removed)),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn commit_note_row(
        index: usize,
        files: Option<&FilesLoad>,
        (graph, lanes): (&GraphRow, usize),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let note = match files {
            Some(FilesLoad::Failed(reason)) => reason.clone(),
            Some(FilesLoad::Ready(_)) => EMPTY_NOTE.to_owned(),
            Some(FilesLoad::Reading) | None => LOADING_NOTE.to_owned(),
        };
        row_base(pal)
            .id((NOTE_ROW_PREFIX, index))
            .child(graph_cell(GraphCell::below(graph), lanes))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .pl(px(ICON_SM + SPACE_1_5))
                    .text_size(px(TEXT_XS))
                    .text_color(rgb(pal.text_muted))
                    .child(note),
            )
            .into_any_element()
    }
}
