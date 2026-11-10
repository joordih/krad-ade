mod actions;
mod history;
mod rows;

use gpui_kit::component::InteractiveElementExt as _;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, uniform_list, AnyElement, ClickEvent, Context, FontWeight, InteractiveElement,
    IntoElement, MouseButton, MouseDownEvent, ParentElement, StatefulInteractiveElement, Styled,
    Window,
};
use krad_domain::{ChangesLayout, FileOpening};
use krad_repo::{FileChange, LineCount};
use std::ops::Range;
use std::path::Path;

use super::super::file_tree::{change_color, change_mark};
use super::super::git_actions::ChangeSide;
use super::super::groups::GroupGit;
use super::super::sidebar_terminals::branch_caption;
use super::super::EnvironmentShell;
use crate::components::{icon, Button, EmptyState, IconName};
use crate::theme::{
    palette, row_fill, Palette, ICON_SM, LINE_SM, LINE_XS, NAV_ROW_H, RADIUS_XS, SPACE_0_5,
    SPACE_1, SPACE_1_5, SPACE_2, TEXT_SM, TEXT_XS,
};
pub(in crate::environment_shell) use actions::ChangesAction;
use actions::{change_menu, folder_menu, side_button, ROW_GROUP, SECTION_GROUP};
pub(in crate::environment_shell) use history::CommitHistory;
use rows::{change_rows, ChangeRow, Sides};

const CHANGES_ID: &str = "side-changes";
const CHANGE_ROW_PREFIX: &str = "side-change-row";
const SECTION_ROW_PREFIX: &str = "side-change-section";
const BTN_BRANCH: &str = "side-changes-branch";
const BTN_LAYOUT: &str = "side-changes-layout";
const BTN_COLLAPSE: &str = "side-changes-collapse";
const BTN_REFRESH: &str = "side-changes-refresh";
const BTN_SESSION: &str = "side-changes-session";
const TOOLTIP_SESSION: &str = "Review session changes";
const BTN_MORE: &str = "side-changes-more";
const TOOLTIP_AS_LIST: &str = "View as list";
const TOOLTIP_AS_TREE: &str = "View as tree";
const TOOLTIP_COLLAPSE: &str = "Collapse all";
const TOOLTIP_REFRESH: &str = "Refresh";
const TOOLTIP_MORE: &str = "More git actions";
const TOOLTIP_BRANCH: &str = "Switch branch";
const STAGED_LABEL: &str = "Staged Changes";
const UNSTAGED_LABEL: &str = "Changes";
const ROW_H: f32 = 26.0;
const INDENT: f32 = 14.0;
const ADDED_SIGN: &str = "+";
const REMOVED_SIGN: &str = "−";
const NO_REPO_TITLE: &str = "No repository";
const NO_REPO_TEXT: &str = "The selected group's folder is not a git repository.";
const CLEAN_TITLE: &str = "No changes";
const CLEAN_TEXT: &str = "The working tree matches the last commit.";

const fn side_label(side: ChangeSide) -> &'static str {
    match side {
        ChangeSide::Staged => STAGED_LABEL,
        ChangeSide::Unstaged => UNSTAGED_LABEL,
    }
}

fn side_changes_of(git: &GroupGit, side: ChangeSide) -> &[FileChange] {
    match side {
        ChangeSide::Staged => &git.status.staged,
        ChangeSide::Unstaged => &git.status.unstaged,
    }
}

fn line_count_chips(lines: LineCount, pal: &Palette) -> Vec<AnyElement> {
    [
        (lines.added, ADDED_SIGN, pal.success_fg),
        (lines.removed, REMOVED_SIGN, pal.error_fg),
    ]
    .into_iter()
    .filter(|(count, _, _)| *count > 0)
    .map(|(count, sign, color)| {
        div()
            .flex_none()
            .text_size(px(TEXT_XS))
            .text_color(rgb(color))
            .child(format!("{sign}{count}"))
            .into_any_element()
    })
    .collect()
}

#[allow(clippy::cast_precision_loss)]
fn indent(depth: usize) -> f32 {
    depth as f32 * INDENT
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn group_git(&self) -> Option<&GroupGit> {
        let root = &self.active_group()?.root;
        self.active_git(root)
    }

    fn changes_layout(&self) -> ChangesLayout {
        self.side_layout()
            .map_or_else(ChangesLayout::default, |layout| layout.changes_layout)
    }

    fn change_rows(&self) -> Vec<ChangeRow> {
        self.group_git().map_or_else(Vec::new, |git| {
            let sides = Sides {
                staged: &git.status.staged,
                unstaged: &git.status.unstaged,
            };
            change_rows(
                &sides,
                self.changes_layout(),
                &self.git_ui.expanded,
                &self.git_ui.collapsed,
            )
        })
    }

    pub(super) fn side_changes(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(git) = self.group_git() else {
            return EmptyState::new(IconName::GitBranch, NO_REPO_TITLE, NO_REPO_TEXT)
                .into_any_element();
        };
        let body = if git.status.changes.is_empty() {
            EmptyState::new(IconName::CheckCircle, CLEAN_TITLE, CLEAN_TEXT).into_any_element()
        } else {
            uniform_list(
                CHANGES_ID,
                self.change_rows().len(),
                cx.processor(|this: &mut Self, range: Range<usize>, _: &mut Window, cx| {
                    let rows = this.change_rows();
                    rows.get(range)
                        .unwrap_or_default()
                        .iter()
                        .map(|row| this.change_row(row, cx))
                        .collect::<Vec<_>>()
                }),
            )
            .flex_1()
            .min_h_0()
            .into_any_element()
        };
        let toolbar = self.changes_toolbar(git, cx);
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(px(SPACE_0_5))
            .p(px(SPACE_1))
            .child(toolbar)
            .child(self.commit_box(cx))
            .child(body)
            .child(self.history_section(cx))
            .into_any_element()
    }

    fn changes_toolbar(&self, git: &GroupGit, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let layout = self.changes_layout();
        let (layout_icon, layout_tooltip) = match layout {
            ChangesLayout::Tree => (IconName::ListView, TOOLTIP_AS_LIST),
            ChangesLayout::List => (IconName::TreeView, TOOLTIP_AS_TREE),
        };
        let branch = div()
            .id(BTN_BRANCH)
            .flex()
            .flex_row()
            .flex_1()
            .min_w_0()
            .items_center()
            .gap(px(SPACE_1))
            .h_full()
            .px(px(SPACE_1))
            .rounded(px(RADIUS_XS))
            .cursor_pointer()
            .hover(|style| style.bg(pal.bg_hover))
            .tooltip(|_, cx| crate::components::tooltip_view(TOOLTIP_BRANCH.into(), cx))
            .on_click(cx.listener(|this: &mut Self, event: &ClickEvent, _, cx| {
                let at = event.position();
                this.open_branch_switcher((at.x.as_f32(), at.y.as_f32()), cx);
            }))
            .child(icon(IconName::GitBranch, ICON_SM, pal.text_muted))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(pal.text_default))
                    .child(branch_caption(git)),
            );
        div()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SPACE_0_5))
            .h(px(NAV_ROW_H))
            .pl(px(SPACE_1))
            .text_size(px(TEXT_XS))
            .line_height(px(LINE_XS))
            .child(branch)
            .child(
                Button::new(BTN_LAYOUT)
                    .icon(layout_icon)
                    .small()
                    .tooltip(layout_tooltip)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.edit_side_layout(cx, |layout| {
                            layout.changes_layout = layout.changes_layout.toggled();
                        });
                    })),
            )
            .when(layout == ChangesLayout::Tree, |bar| {
                bar.child(
                    Button::new(BTN_COLLAPSE)
                        .icon(IconName::CollapseAll)
                        .small()
                        .tooltip(TOOLTIP_COLLAPSE)
                        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                            this.git_ui.expanded.clear();
                            cx.notify();
                        })),
                )
            })
            .child(
                Button::new(BTN_SESSION)
                    .icon(IconName::Tasks)
                    .small()
                    .tooltip(TOOLTIP_SESSION)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.open_session_changes(cx);
                    })),
            )
            .child(
                Button::new(BTN_REFRESH)
                    .icon(IconName::Refresh)
                    .small()
                    .tooltip(TOOLTIP_REFRESH)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| this.refresh_git(cx))),
            )
            .child(
                Button::new(BTN_MORE)
                    .icon(IconName::More)
                    .small()
                    .tooltip(TOOLTIP_MORE)
                    .on_click(cx.listener(|this: &mut Self, event: &ClickEvent, _, cx| {
                        let at = event.position();
                        this.open_git_menu((at.x.as_f32(), at.y.as_f32()), cx);
                    })),
            )
            .into_any_element()
    }

    fn change_row(&self, row: &ChangeRow, cx: &mut Context<Self>) -> AnyElement {
        match row {
            ChangeRow::Section {
                side,
                count,
                collapsed,
            } => Self::change_section_row(*side, *count, *collapsed, cx),
            ChangeRow::Folder {
                side,
                path,
                label,
                depth,
                collapsed,
            } => Self::change_folder_row(*side, path, label, *depth, *collapsed, cx),
            ChangeRow::File {
                side,
                change,
                depth,
            } => self.change_file_row(*side, *change, *depth, cx),
        }
    }

    fn change_section_row(
        side: ChangeSide,
        count: usize,
        collapsed: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let bulk = side_button(SECTION_GROUP, side, None, cx);
        div()
            .id((SECTION_ROW_PREFIX, row_key(side, Path::new(""))))
            .group(SECTION_GROUP)
            .flex()
            .flex_row()
            .w_full()
            .items_center()
            .gap(px(SPACE_1_5))
            .h(px(ROW_H))
            .px(px(SPACE_2))
            .rounded(px(RADIUS_XS))
            .cursor_pointer()
            .hover(|style| style.bg(pal.bg_hover))
            .text_size(px(TEXT_XS))
            .line_height(px(LINE_XS))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(pal.text_muted))
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                if !this.git_ui.collapsed.remove(&side) {
                    this.git_ui.collapsed.insert(side);
                }
                cx.notify();
            }))
            .child(icon(
                if collapsed {
                    IconName::ChevronRight
                } else {
                    IconName::ChevronDown
                },
                ICON_SM,
                pal.text_subtle,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(side_label(side).to_uppercase()),
            )
            .child(bulk)
            .child(div().flex_none().child(count.to_string()))
            .into_any_element()
    }

    fn change_folder_row(
        side: ChangeSide,
        path: &Path,
        label: &str,
        depth: usize,
        collapsed: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let toggled = path.to_path_buf();
        let menu_folder = path.to_path_buf();
        let discard = (side == ChangeSide::Unstaged).then(|| Self::discard_folder_button(path, cx));
        div()
            .id((CHANGE_ROW_PREFIX, row_key(side, path)))
            .group(ROW_GROUP)
            .flex()
            .flex_row()
            .w_full()
            .items_center()
            .gap(px(SPACE_1_5))
            .h(px(ROW_H))
            .pl(px(SPACE_2 + indent(depth + 1)))
            .pr(px(SPACE_2))
            .rounded(px(RADIUS_XS))
            .cursor_pointer()
            .hover(|style| style.bg(pal.bg_hover))
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .text_color(rgb(pal.text_default))
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                if !this.git_ui.expanded.remove(&toggled) {
                    this.git_ui.expanded.insert(toggled.clone());
                }
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this: &mut Self, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.open_menu_at(event, folder_menu(&menu_folder), cx);
                }),
            )
            .child(icon(
                if collapsed {
                    IconName::ChevronRight
                } else {
                    IconName::ChevronDown
                },
                ICON_SM,
                pal.text_subtle,
            ))
            .child(icon(IconName::Folder, ICON_SM, pal.text_muted))
            .child(div().flex_1().min_w_0().truncate().child(label.to_owned()))
            .children(discard)
            .into_any_element()
    }

    fn change_file_row(
        &self,
        side: ChangeSide,
        index: usize,
        depth: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let Some(git) = self.group_git() else {
            return div().into_any_element();
        };
        let Some(change) = side_changes_of(git, side).get(index) else {
            return div().into_any_element();
        };
        let path = git.repo_root.join(&change.path);
        let lines = git.lines.get(&change.path).copied().unwrap_or_default();
        let name = change
            .path
            .file_name()
            .map_or_default(|name| name.to_string_lossy().into_owned());
        let folder = (self.changes_layout() == ChangesLayout::List)
            .then(|| {
                change
                    .path
                    .parent()
                    .map(|parent| parent.display().to_string())
            })
            .flatten()
            .filter(|folder| !folder.is_empty());
        let shown = self.shown_changes_path().as_ref() == Some(&path);
        let color = change_color(change.kind, pal);
        let opened = path.clone();
        let pinned = path.clone();
        let menu = change_menu(&git.repo_root, change);
        let discard = (side == ChangeSide::Unstaged)
            .then(|| Self::discard_button(git.repo_root.clone(), change, cx));
        let staging = side_button(ROW_GROUP, side, Some(change.path.clone()), cx);
        div()
            .id((CHANGE_ROW_PREFIX, row_key(side, &path)))
            .group(ROW_GROUP)
            .flex()
            .flex_row()
            .w_full()
            .items_center()
            .gap(px(SPACE_1_5))
            .h(px(ROW_H))
            .pl(px(SPACE_2 + indent(depth + 1) + ICON_SM + SPACE_1_5))
            .pr(px(SPACE_2))
            .rounded(px(RADIUS_XS))
            .cursor_pointer()
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .when(shown, |row| row.bg(row_fill(pal, cx)))
            .when(!shown, |row| row.hover(|style| style.bg(pal.bg_hover)))
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.open_changes_tab(&opened, FileOpening::Preview, cx);
            }))
            .on_double_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.open_changes_tab(&pinned, FileOpening::Pinned, cx);
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this: &mut Self, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.open_menu_at(event, menu.clone(), cx);
                }),
            )
            .child(icon(IconName::File, ICON_SM, pal.text_muted))
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
            .children(line_count_chips(lines, pal))
            .children(discard)
            .child(staging)
            .child(
                div()
                    .flex_none()
                    .w(px(ICON_SM))
                    .text_size(px(TEXT_XS))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(color))
                    .child(change_mark(change.kind)),
            )
            .into_any_element()
    }
}

fn row_key(side: ChangeSide, path: &Path) -> u64 {
    use std::hash::{Hash as _, Hasher as _};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    side.hash(&mut hasher);
    path.hash(&mut hasher);
    hasher.finish()
}
