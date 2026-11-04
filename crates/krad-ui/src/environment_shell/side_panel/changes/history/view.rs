use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, uniform_list, AnyElement, Context, FontWeight, InteractiveElement, IntoElement,
    MouseButton, MouseDownEvent, ParentElement, StatefulInteractiveElement, Styled, Window,
};
use std::ops::Range;

use super::super::super::super::drag::DragState;
use super::super::super::super::EnvironmentShell;
use super::super::super::SidePanelDrag;
use super::{history_rows, HistoryLoad, HistoryRow};
use crate::components::{icon, Button, IconName};
use crate::theme::{
    palette, ICON_SM, LINE_XS, NAV_ROW_H, SPACE_0_5, SPACE_1, SPACE_1_5, SPACE_2, TEXT_XS,
};

const HISTORY_ID: &str = "side-history";
const HEADER_ID: &str = "side-history-header";
const BTN_REFRESH: &str = "side-history-refresh";
const HISTORY_LABEL: &str = "History";
const TOOLTIP_REFRESH: &str = "Reload the history";
const LOADING_TEXT: &str = "Loading…";
const NO_COMMITS_TEXT: &str = "No commits yet.";
const HANDLE_H: f32 = 6.0;

impl EnvironmentShell {
    pub(in crate::environment_shell::side_panel) fn history_section(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let (open, height) = self.side_layout().map_or((false, 0.0), |layout| {
            (layout.history_open, layout.history_height)
        });
        div()
            .relative()
            .flex()
            .flex_col()
            .flex_initial()
            .min_h_0()
            .border_t_1()
            .border_color(pal.divider)
            .pt(px(SPACE_0_5))
            .child(self.history_header(open, cx))
            .when(open, |section| {
                section
                    .h(px(height))
                    .child(self.history_body(cx))
                    .child(Self::history_handle(height, cx))
            })
            .into_any_element()
    }

    fn history_header(&self, open: bool, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let count = match &self.history.load {
            HistoryLoad::Ready(read) if open => Some(read.commits.len()),
            _ => None,
        };
        let title = count.map_or_else(
            || HISTORY_LABEL.to_uppercase(),
            |count| format!("{} · {count}", HISTORY_LABEL.to_uppercase()),
        );
        div()
            .id(HEADER_ID)
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SPACE_1_5))
            .h(px(NAV_ROW_H))
            .px(px(SPACE_2))
            .cursor_pointer()
            .text_size(px(TEXT_XS))
            .line_height(px(LINE_XS))
            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                this.edit_side_layout(cx, |layout| layout.history_open = !layout.history_open);
            }))
            .child(icon(
                if open {
                    IconName::ChevronDown
                } else {
                    IconName::ChevronRight
                },
                ICON_SM,
                pal.text_subtle,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(pal.text_muted))
                    .child(title),
            )
            .when(open, |header| {
                header.child(
                    Button::new(BTN_REFRESH)
                        .icon(IconName::Refresh)
                        .small()
                        .tooltip(TOOLTIP_REFRESH)
                        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                            this.history.read_for = None;
                            cx.notify();
                        })),
                )
            })
            .into_any_element()
    }

    fn history_note(text: String, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        div()
            .px(px(SPACE_2))
            .py(px(SPACE_1))
            .text_size(px(TEXT_XS))
            .text_color(rgb(pal.text_muted))
            .child(text)
            .into_any_element()
    }

    fn history_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let count = match &self.history.load {
            HistoryLoad::Idle | HistoryLoad::Reading => {
                return Self::history_note(LOADING_TEXT.to_owned(), cx);
            }
            HistoryLoad::Failed(reason) => return Self::history_note(reason.clone(), cx),
            HistoryLoad::Ready(read) if read.commits.is_empty() => {
                return Self::history_note(NO_COMMITS_TEXT.to_owned(), cx);
            }
            HistoryLoad::Ready(read) => history_rows(&read.commits, &self.history.expanded).len(),
        };
        uniform_list(
            HISTORY_ID,
            count,
            cx.processor(|this: &mut Self, range: Range<usize>, _: &mut Window, cx| {
                this.history_items(range, cx)
            }),
        )
        .flex_1()
        .min_h_0()
        .px(px(SPACE_1))
        .into_any_element()
    }

    fn history_items(&self, range: Range<usize>, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let HistoryLoad::Ready(read) = &self.history.load else {
            return Vec::new();
        };
        let Some(repo) = self.history_repo() else {
            return Vec::new();
        };
        let rows = history_rows(&read.commits, &self.history.expanded);
        let lanes = read.lanes;
        range
            .filter_map(|row| {
                let item = rows.get(row)?;
                let commit_index = match item {
                    HistoryRow::Commit(index) | HistoryRow::Note(index) => *index,
                    HistoryRow::File { commit, .. } => *commit,
                };
                let commit = read.commits.get(commit_index)?;
                let graph = read.graph.get(commit_index)?;
                let files = self.history.expanded.get(&commit.sha);
                Some(match item {
                    HistoryRow::Commit(index) => {
                        Self::commit_row(*index, commit, graph, (lanes, files.is_some()), cx)
                    }
                    HistoryRow::Note(index) => {
                        Self::commit_note_row(*index, files, (graph, lanes), cx)
                    }
                    HistoryRow::File { file, .. } => {
                        let super::FilesLoad::Ready(files) = files? else {
                            return None;
                        };
                        Self::commit_file_row(
                            row,
                            commit,
                            files.get(*file)?,
                            (graph, lanes),
                            repo,
                            cx,
                        )
                    }
                })
            })
            .collect()
    }

    fn history_handle(height: f32, cx: &mut Context<Self>) -> AnyElement {
        div()
            .absolute()
            .left_0()
            .right_0()
            .top(px(-HANDLE_H / 2.0))
            .h(px(HANDLE_H))
            .cursor_row_resize()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this: &mut Self, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.drag = DragState::SidePanel(SidePanelDrag::History {
                        mouse_start: event.position.y.as_f32(),
                        height_start: height,
                    });
                    cx.notify();
                }),
            )
            .into_any_element()
    }
}
