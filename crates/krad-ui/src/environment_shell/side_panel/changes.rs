use gpui_kit::component::InteractiveElementExt as _;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, uniform_list, AnyElement, Context, FontWeight, InteractiveElement, IntoElement,
    ParentElement, StatefulInteractiveElement, Styled, Window,
};
use krad_domain::{ChangesLayout, FileOpening};
use krad_repo::FileChange;
use std::collections::{BTreeMap, HashSet};
use std::ops::Range;
use std::path::{Path, PathBuf};

use super::super::file_tree::{change_color, change_mark};
use super::super::groups::GroupGit;
use super::super::EnvironmentShell;
use crate::components::{icon, Button, EmptyState, IconName};
use crate::theme::{
    palette, row_fill, ICON_SM, LINE_SM, LINE_XS, NAV_ROW_H, RADIUS_XS, SPACE_0_5, SPACE_1,
    SPACE_1_5, SPACE_2, TEXT_SM, TEXT_XS,
};

const CHANGES_ID: &str = "side-changes";
const CHANGE_ROW_PREFIX: &str = "side-change-row";
const BTN_LAYOUT: &str = "side-changes-layout";
const BTN_COLLAPSE: &str = "side-changes-collapse";
const BTN_REFRESH: &str = "side-changes-refresh";
const TOOLTIP_AS_LIST: &str = "View as list";
const TOOLTIP_AS_TREE: &str = "View as tree";
const TOOLTIP_COLLAPSE: &str = "Collapse all";
const TOOLTIP_REFRESH: &str = "Refresh";
const CHANGES_LABEL: &str = "Changes";
const ROW_H: f32 = 26.0;
const INDENT: f32 = 14.0;
const FOLDER_SEPARATOR: &str = "/";
const ADDED_SIGN: &str = "+";
const REMOVED_SIGN: &str = "−";
const NO_REPO_TITLE: &str = "No repository";
const NO_REPO_TEXT: &str = "The selected group's folder is not a git repository.";
const CLEAN_TITLE: &str = "No changes";
const CLEAN_TEXT: &str = "The working tree matches the last commit.";

#[derive(Clone, Debug, PartialEq, Eq)]
enum ChangeRow {
    Folder {
        path: PathBuf,
        label: String,
        depth: usize,
        collapsed: bool,
    },
    File {
        change: usize,
        depth: usize,
    },
}

#[derive(Default)]
struct FolderNode {
    folders: BTreeMap<String, FolderNode>,
    files: Vec<(String, usize)>,
}

impl FolderNode {
    fn insert(&mut self, path: &Path, change: usize) {
        let parts: Vec<String> = path
            .iter()
            .map(|part| part.to_string_lossy().into_owned())
            .collect();
        let Some((name, folders)) = parts.split_last() else {
            return;
        };
        let mut node = self;
        for folder in folders {
            node = node.folders.entry(folder.clone()).or_default();
        }
        node.files.push((name.clone(), change));
    }

    fn push_rows(
        &self,
        parent: &Path,
        depth: usize,
        expanded: &HashSet<PathBuf>,
        rows: &mut Vec<ChangeRow>,
    ) {
        for (name, folder) in &self.folders {
            let mut label = name.clone();
            let mut path = parent.join(name);
            let mut node = folder;
            while node.files.is_empty() && node.folders.len() == 1 {
                let Some((next_name, next)) = node.folders.iter().next() else {
                    break;
                };
                label = format!("{label}{FOLDER_SEPARATOR}{next_name}");
                path = path.join(next_name);
                node = next;
            }
            let shut = !expanded.contains(&path);
            rows.push(ChangeRow::Folder {
                path: path.clone(),
                label,
                depth,
                collapsed: shut,
            });
            if !shut {
                node.push_rows(&path, depth + 1, expanded, rows);
            }
        }
        let mut files = self.files.clone();
        files.sort();
        rows.extend(
            files
                .into_iter()
                .map(|(_, change)| ChangeRow::File { change, depth }),
        );
    }
}

fn change_rows(
    changes: &[FileChange],
    layout: ChangesLayout,
    expanded: &HashSet<PathBuf>,
) -> Vec<ChangeRow> {
    match layout {
        ChangesLayout::List => {
            let mut order: Vec<usize> = (0..changes.len()).collect();
            order.sort_by(|a, b| changes[*a].path.cmp(&changes[*b].path));
            order
                .into_iter()
                .map(|change| ChangeRow::File { change, depth: 0 })
                .collect()
        }
        ChangesLayout::Tree => {
            let mut root = FolderNode::default();
            for (index, change) in changes.iter().enumerate() {
                root.insert(&change.path, index);
            }
            let mut rows = Vec::new();
            root.push_rows(Path::new(""), 0, expanded, &mut rows);
            rows
        }
    }
}

#[allow(clippy::cast_precision_loss)]
fn indent(depth: usize) -> f32 {
    depth as f32 * INDENT
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn group_git(&self) -> Option<&GroupGit> {
        let root = &self.active_group()?.root;
        self.git.get(root)
    }

    fn changes_layout(&self) -> ChangesLayout {
        self.side_layout()
            .map_or_else(ChangesLayout::default, |layout| layout.changes_layout)
    }

    fn change_rows(&self) -> Vec<ChangeRow> {
        self.group_git().map_or_else(Vec::new, |git| {
            change_rows(
                &git.status.changes,
                self.changes_layout(),
                &self.changes_expanded,
            )
        })
    }

    pub(super) fn side_changes(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(git) = self.group_git() else {
            return EmptyState::new(IconName::GitBranch, NO_REPO_TITLE, NO_REPO_TEXT)
                .into_any_element();
        };
        let count = git.status.changes.len();
        let body = if count == 0 {
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
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(px(SPACE_0_5))
            .p(px(SPACE_1))
            .child(self.changes_toolbar(count, cx))
            .child(body)
            .into_any_element()
    }

    fn changes_toolbar(&self, count: usize, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let layout = self.changes_layout();
        let (layout_icon, layout_tooltip) = match layout {
            ChangesLayout::Tree => (IconName::ListView, TOOLTIP_AS_LIST),
            ChangesLayout::List => (IconName::TreeView, TOOLTIP_AS_TREE),
        };
        div()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SPACE_0_5))
            .h(px(NAV_ROW_H))
            .pl(px(SPACE_2))
            .text_size(px(TEXT_XS))
            .line_height(px(LINE_XS))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(pal.text_muted))
                    .child(format!("{} · {count}", CHANGES_LABEL.to_uppercase())),
            )
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
                            this.changes_expanded.clear();
                            cx.notify();
                        })),
                )
            })
            .child(
                Button::new(BTN_REFRESH)
                    .icon(IconName::Refresh)
                    .small()
                    .tooltip(TOOLTIP_REFRESH)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| this.refresh_git(cx))),
            )
            .into_any_element()
    }

    fn change_row(&self, row: &ChangeRow, cx: &mut Context<Self>) -> AnyElement {
        match row {
            ChangeRow::Folder {
                path,
                label,
                depth,
                collapsed,
            } => Self::change_folder_row(path, label, *depth, *collapsed, cx),
            ChangeRow::File { change, depth } => self.change_file_row(*change, *depth, cx),
        }
    }

    fn change_folder_row(
        path: &Path,
        label: &str,
        depth: usize,
        collapsed: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let toggled = path.to_path_buf();
        div()
            .id((CHANGE_ROW_PREFIX, row_key(path)))
            .flex()
            .flex_row()
            .w_full()
            .items_center()
            .gap(px(SPACE_1_5))
            .h(px(ROW_H))
            .pl(px(SPACE_2 + indent(depth)))
            .pr(px(SPACE_2))
            .rounded(px(RADIUS_XS))
            .cursor_pointer()
            .hover(|style| style.bg(pal.bg_hover))
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .text_color(rgb(pal.text_default))
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                if !this.changes_expanded.remove(&toggled) {
                    this.changes_expanded.insert(toggled.clone());
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
            .child(icon(IconName::Folder, ICON_SM, pal.text_muted))
            .child(div().flex_1().min_w_0().truncate().child(label.to_owned()))
            .into_any_element()
    }

    fn change_file_row(&self, index: usize, depth: usize, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let Some(git) = self.group_git() else {
            return div().into_any_element();
        };
        let Some(change) = git.status.changes.get(index) else {
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
        div()
            .id((CHANGE_ROW_PREFIX, row_key(&path)))
            .flex()
            .flex_row()
            .w_full()
            .items_center()
            .gap(px(SPACE_1_5))
            .h(px(ROW_H))
            .pl(px(SPACE_2 + indent(depth) + ICON_SM + SPACE_1_5))
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
            .when(lines.added > 0, |row| {
                row.child(
                    div()
                        .flex_none()
                        .text_size(px(TEXT_XS))
                        .text_color(rgb(pal.success_fg))
                        .child(format!("{ADDED_SIGN}{}", lines.added)),
                )
            })
            .when(lines.removed > 0, |row| {
                row.child(
                    div()
                        .flex_none()
                        .text_size(px(TEXT_XS))
                        .text_color(rgb(pal.error_fg))
                        .child(format!("{REMOVED_SIGN}{}", lines.removed)),
                )
            })
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

fn row_key(path: &Path) -> u64 {
    use std::hash::{Hash as _, Hasher as _};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_repo::ChangeKind;

    fn changes(paths: &[&str]) -> Vec<FileChange> {
        paths
            .iter()
            .map(|path| FileChange {
                path: PathBuf::from(path),
                kind: ChangeKind::Modified,
            })
            .collect()
    }

    fn shown(rows: &[ChangeRow], changes: &[FileChange]) -> Vec<(String, usize)> {
        rows.iter()
            .map(|row| match row {
                ChangeRow::Folder { label, depth, .. } => (format!("{label}/"), *depth),
                ChangeRow::File { change, depth } => {
                    (changes[*change].path.to_string_lossy().into_owned(), *depth)
                }
            })
            .collect()
    }

    #[test]
    fn the_tree_puts_folders_first_and_compacts_single_folder_chains() {
        let changes = changes(&[
            "README.md",
            "crates/ui/src/a.rs",
            "crates/ui/src/b.rs",
            "Cargo.toml",
        ]);
        let folded = change_rows(&changes, ChangesLayout::Tree, &HashSet::new());
        assert_eq!(
            shown(&folded, &changes),
            [
                ("crates/ui/src/".to_owned(), 0),
                ("Cargo.toml".to_owned(), 0),
                ("README.md".to_owned(), 0),
            ],
            "folders start folded"
        );
        let expanded: HashSet<PathBuf> = [PathBuf::from("crates/ui/src")].into();
        let rows = change_rows(&changes, ChangesLayout::Tree, &expanded);
        assert_eq!(
            shown(&rows, &changes),
            [
                ("crates/ui/src/".to_owned(), 0),
                ("crates/ui/src/a.rs".to_owned(), 1),
                ("crates/ui/src/b.rs".to_owned(), 1),
                ("Cargo.toml".to_owned(), 0),
                ("README.md".to_owned(), 0),
            ]
        );
    }

    #[test]
    fn only_opened_folders_show_their_files_and_the_list_sorts_by_path() {
        let changes = changes(&["src/b.rs", "docs/a.md", "src/a.rs"]);
        let expanded: HashSet<PathBuf> = [PathBuf::from("docs")].into();
        let rows = change_rows(&changes, ChangesLayout::Tree, &expanded);
        assert_eq!(
            shown(&rows, &changes),
            [
                ("docs/".to_owned(), 0),
                ("docs/a.md".to_owned(), 1),
                ("src/".to_owned(), 0),
            ]
        );
        let list = change_rows(&changes, ChangesLayout::List, &HashSet::new());
        assert_eq!(
            shown(&list, &changes),
            [
                ("docs/a.md".to_owned(), 0),
                ("src/a.rs".to_owned(), 0),
                ("src/b.rs".to_owned(), 0),
            ]
        );
    }
}
