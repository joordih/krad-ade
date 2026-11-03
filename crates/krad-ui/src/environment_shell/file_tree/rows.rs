use gpui_kit::component::InteractiveElementExt as _;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, uniform_list, AnyElement, App, AppContext as _, Context, Div, ExternalPaths,
    FontWeight, InteractiveElement, IntoElement, MouseButton, MouseDownEvent, ParentElement,
    Stateful, StatefulInteractiveElement, Styled, Window,
};
use krad_domain::{FileOpening, FolderKind, ProblemSeverity};
use krad_repo::{ChangeKind, DirEntry, EntryKind};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::{Path, PathBuf};

use super::super::EnvironmentShell;
use super::{
    ChangeMarks, NewEntryRow, TreeRow, DRAFT_ROW_ID, FILE_ROW_H, FILE_ROW_PREFIX, FILE_TREE_ID,
    INDENT, MARK_ADDED, MARK_CONFLICTED, MARK_DELETED, MARK_MODIFIED, MARK_RENAMED, MARK_UNTRACKED,
};
use crate::actions::{RenameFile, FILE_TREE_KEY_CONTEXT};
use crate::components::{icon, IconName};
use crate::theme::{
    appearance, palette, row_fill, row_shadow, Appearance, Palette, ICON_SM, LINE_SM, LINE_XS,
    RADIUS_XS, SPACE_1_5, SPACE_2, TEXT_SM, TEXT_XS,
};

pub(in crate::environment_shell) const fn change_mark(kind: ChangeKind) -> &'static str {
    match kind {
        ChangeKind::Modified => MARK_MODIFIED,
        ChangeKind::Added => MARK_ADDED,
        ChangeKind::Deleted => MARK_DELETED,
        ChangeKind::Renamed => MARK_RENAMED,
        ChangeKind::Untracked => MARK_UNTRACKED,
        ChangeKind::Conflicted => MARK_CONFLICTED,
    }
}

pub(in crate::environment_shell) const fn change_color(kind: ChangeKind, pal: &Palette) -> u32 {
    match kind {
        ChangeKind::Modified | ChangeKind::Renamed => pal.warning_fg,
        ChangeKind::Added | ChangeKind::Untracked => pal.success_fg,
        ChangeKind::Deleted | ChangeKind::Conflicted => pal.error_fg,
    }
}

impl EnvironmentShell {
    pub(super) fn change_marks(&self) -> ChangeMarks {
        let mut marks = ChangeMarks {
            files: HashMap::new(),
            folders: HashSet::new(),
        };
        let Some(git) = self
            .file_tree
            .group_root
            .as_ref()
            .and_then(|root| self.active_git(root))
        else {
            return marks;
        };
        for change in &git.status.changes {
            let path = git.repo_root.join(&change.path);
            marks
                .folders
                .extend(path.ancestors().skip(1).map(Path::to_path_buf));
            marks.files.insert(path, change.kind);
        }
        marks
    }

    pub(super) fn tree_rows(&self) -> Vec<TreeRow> {
        let draft = self
            .new_entry_draft()
            .map(|(dir, kind, label)| NewEntryRow { dir, kind, label });
        self.file_tree.rows(draft.as_ref(), self.language_catalog())
    }

    pub(in crate::environment_shell) fn file_tree_list(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let count = self.tree_rows().len();
        let editable = true;
        let list = uniform_list(
            FILE_TREE_ID,
            count,
            cx.processor(|this: &mut Self, range: Range<usize>, _: &mut Window, cx| {
                let rows = this.tree_rows();
                let marks = this.change_marks();
                rows.get(range)
                    .unwrap_or_default()
                    .iter()
                    .map(|row| match row {
                        TreeRow::Entry {
                            entry,
                            label,
                            depth,
                            expanded,
                        } => {
                            this.file_row(entry, label.as_deref(), (*depth, *expanded), &marks, cx)
                        }
                        TreeRow::Draft { kind, label, depth } => {
                            Self::draft_row(*kind, label, *depth, cx)
                        }
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .flex_1()
        .min_h_0();
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .track_focus(&self.file_tree_focus)
            .key_context(FILE_TREE_KEY_CONTEXT)
            .when(editable, |tree| {
                tree.on_action(cx.listener(Self::on_rename_file))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|this: &mut Self, event: &MouseDownEvent, _, cx| {
                            if let Some(root) = this.file_tree.root.clone() {
                                this.open_menu_at(event, Self::folder_menu_entries(&root), cx);
                            }
                        }),
                    )
                    .on_drop(cx.listener(
                        |this: &mut Self, dragged: &super::drag::DraggedFile, _, cx| {
                            if let Some(root) = this.file_tree.root.clone() {
                                this.move_file(dragged.path.clone(), root, cx);
                            }
                        },
                    ))
                    .on_drop(
                        cx.listener(|this: &mut Self, dropped: &ExternalPaths, window, cx| {
                            if let Some(root) = this.file_tree.root.clone() {
                                this.drop_on_tree(root, dropped.paths(), window, cx);
                            }
                        }),
                    )
            })
            .children(self.upload_bar(cx))
            .child(list)
            .into_any_element()
    }

    pub(super) fn on_rename_file(
        &mut self,
        _: &RenameFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(path) = self.file_tree.selected.clone() {
            window.focus(&self.focus, cx);
            self.start_file_rename(path, cx);
        }
    }

    pub(super) fn draft_row(
        kind: EntryKind,
        label: &str,
        depth: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let glyph = match kind {
            EntryKind::Directory => IconName::Folder,
            EntryKind::File => IconName::File,
        };
        div()
            .id(DRAFT_ROW_ID)
            .flex()
            .flex_row()
            .w_full()
            .items_center()
            .gap(px(SPACE_1_5))
            .h(px(FILE_ROW_H))
            .pl(px(SPACE_2 + indent(depth)))
            .pr(px(SPACE_2))
            .rounded(px(RADIUS_XS))
            .bg(row_fill(pal, cx))
            .shadow(row_shadow(pal, cx))
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .text_color(rgb(pal.text_strong))
            .child(div().flex_none().size(px(ICON_SM)))
            .child(icon(glyph, ICON_SM, pal.text_muted))
            .child(div().flex_1().min_w_0().truncate().child(label.to_owned()))
            .into_any_element()
    }

    pub(super) fn with_entry_gestures(
        row: Stateful<Div>,
        entry: &DirEntry,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let is_dir = entry.kind == EntryKind::Directory;
        let drop_dir = if is_dir {
            Some(entry.path.clone())
        } else {
            entry.path.parent().map(Path::to_path_buf)
        };
        let menu_entry = entry.clone();
        let dragged = super::drag::DraggedFile::new(entry);
        row.on_mouse_down(
            MouseButton::Right,
            cx.listener(move |this: &mut Self, event: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.file_tree.selected = Some(menu_entry.path.clone());
                this.open_menu_at(event, Self::file_menu_entries(&menu_entry), cx);
            }),
        )
        .on_drag(dragged, |dragged, _, _, cx| cx.new(|_| dragged.clone()))
        .when(is_dir, |row| {
            row.drag_over::<super::drag::DraggedFile>(|style, _, _, cx| {
                style.bg(palette(cx).bg_selected)
            })
        })
        .on_drop(cx.listener({
            let drop_dir = drop_dir.clone();
            move |this: &mut Self, dragged: &super::drag::DraggedFile, _, cx| {
                if let Some(dir) = drop_dir.clone() {
                    this.move_file(dragged.path.clone(), dir, cx);
                }
            }
        }))
        .on_drop(cx.listener(
            move |this: &mut Self, dropped: &ExternalPaths, window, cx| {
                if let Some(dir) = drop_dir.clone() {
                    this.drop_on_tree(dir, dropped.paths(), window, cx);
                }
            },
        ))
    }

    pub(super) fn with_open_gestures(
        row: Stateful<Div>,
        path: PathBuf,
        is_dir: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let pinned = path.clone();
        row.on_click(cx.listener(move |this: &mut Self, _, window, cx| {
            this.file_tree.selected = Some(path.clone());
            window.focus(&this.file_tree_focus, cx);
            if is_dir {
                this.toggle_dir(path.clone(), cx);
            } else {
                this.open_file_tab(&path, FileOpening::Preview, cx);
            }
        }))
        .when(!is_dir, |row| {
            row.on_double_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.open_file_tab(&pinned, FileOpening::Pinned, cx);
            }))
        })
    }

    fn entry_icon(&self, entry: &DirEntry, expanded: bool, cx: &App) -> AnyElement {
        let is_dir = entry.kind == EntryKind::Directory;
        let dark = appearance(cx) == Appearance::Dark;
        let colors = self.settings.file_icon_colors.palette(dark);
        let colored = self.file_icons.as_ref().and_then(|icons| {
            if !is_dir {
                return icons.file(&entry.name, colors, ICON_SM);
            }
            let kind = if self.language_catalog().in_packages(&entry.path) {
                FolderKind::Package
            } else {
                FolderKind::Plain
            };
            icons.folder(&entry.name, (kind, expanded), colors, ICON_SM)
        });
        colored.unwrap_or_else(|| {
            let glyph = if is_dir {
                IconName::Folder
            } else {
                IconName::File
            };
            icon(glyph, ICON_SM, palette(cx).text_muted).into_any_element()
        })
    }

    pub(super) fn file_row(
        &self,
        entry: &DirEntry,
        compacted: Option<&str>,
        (depth, expanded): (usize, bool),
        marks: &ChangeMarks,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let shown_file = self.shown_file_path();
        let path = entry.path.clone();
        let is_dir = entry.kind == EntryKind::Directory;
        let open = self
            .viewer
            .as_ref()
            .is_some_and(|viewer| viewer.path == entry.path)
            || shown_file.as_ref() == Some(&entry.path);
        let highlighted = open || self.file_tree.selected.as_ref() == Some(&entry.path);
        let change = marks.files.get(&entry.path).copied();
        let folder_changed = is_dir && marks.folders.contains(&entry.path);
        let problem = self
            .worst_problem_at(&entry.path)
            .and_then(|severity| problem_color(severity, pal));
        let label = self
            .file_rename_label(&entry.path)
            .unwrap_or_else(|| compacted.map_or_else(|| entry.name.clone(), str::to_owned));
        let name_color = match change {
            Some(kind) => change_color(kind, pal),
            None if open => pal.text_strong,
            None => pal.text_default,
        };
        let glyph = self.entry_icon(entry, expanded, cx);
        let chevron = is_dir.then(|| {
            icon(
                if expanded {
                    IconName::ChevronDown
                } else {
                    IconName::ChevronRight
                },
                ICON_SM,
                pal.text_subtle,
            )
        });
        let row = div().id((FILE_ROW_PREFIX, row_key(&entry.path)));
        let row = Self::with_entry_gestures(row, entry, cx);
        let row = Self::with_open_gestures(row, path, is_dir, cx);
        row.flex()
            .flex_row()
            .w_full()
            .items_center()
            .gap(px(SPACE_1_5))
            .h(px(FILE_ROW_H))
            .pl(px(SPACE_2 + indent(depth)))
            .pr(px(SPACE_2))
            .rounded(px(RADIUS_XS))
            .cursor_pointer()
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .text_color(rgb(name_color))
            .when(highlighted, |row| {
                row.bg(row_fill(pal, cx)).shadow(row_shadow(pal, cx))
            })
            .when(!highlighted, |row| {
                row.hover(|style| style.bg(pal.bg_hover))
            })
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(px(ICON_SM))
                    .children(chevron),
            )
            .child(glyph)
            .child(entry_name(label, problem))
            .when_some(change, |row, kind| {
                row.child(
                    div()
                        .flex_none()
                        .text_size(px(TEXT_XS))
                        .line_height(px(LINE_XS))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(change_color(kind, pal)))
                        .child(change_mark(kind)),
                )
            })
            .when(folder_changed, |row| {
                row.child(
                    div()
                        .flex_none()
                        .size(px(SPACE_1_5))
                        .rounded_full()
                        .bg(rgb(pal.warning_fg)),
                )
            })
            .into_any_element()
    }
}

fn entry_name(label: impl IntoElement, problem: Option<u32>) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .truncate()
        .when_some(problem, |name, color| {
            name.text_decoration_1()
                .text_decoration_wavy()
                .text_decoration_color(rgb(color))
        })
        .child(label)
}

const fn problem_color(severity: ProblemSeverity, pal: &Palette) -> Option<u32> {
    match severity {
        ProblemSeverity::Error => Some(pal.error_fg),
        ProblemSeverity::Warning => Some(pal.warning_fg),
        ProblemSeverity::Info | ProblemSeverity::Hint => None,
    }
}

#[allow(clippy::cast_precision_loss)]
pub(super) fn indent(depth: usize) -> f32 {
    depth as f32 * INDENT
}

pub(super) fn row_key(path: &Path) -> u64 {
    use std::hash::{Hash as _, Hasher as _};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    hasher.finish()
}
