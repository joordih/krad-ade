mod delete_dialog;
mod drag;
mod ops;

use gpui_kit::component::InteractiveElementExt as _;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, uniform_list, AnyElement, AppContext as _, Context, Div, FontWeight,
    InteractiveElement, IntoElement, MouseButton, MouseDownEvent, ParentElement, Stateful,
    StatefulInteractiveElement, Styled, Window,
};
use krad_domain::FileOpening;
use krad_repo::{ChangeKind, DirEntry, EntryKind};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::{Path, PathBuf};

use super::EnvironmentShell;
use crate::actions::{RenameFile, FILE_TREE_KEY_CONTEXT};
use crate::components::{icon, IconName};
use crate::theme::{
    palette, row_fill, row_shadow, Palette, ICON_SM, LINE_SM, LINE_XS, RADIUS_XS, SPACE_1_5,
    SPACE_2, TEXT_SM, TEXT_XS,
};

const FILE_TREE_ID: &str = "file-tree";
const FILE_ROW_PREFIX: &str = "file-row";
const DRAFT_ROW_ID: &str = "file-draft-row";
const FILE_ROW_H: f32 = 26.0;
const INDENT: f32 = 14.0;
const MARK_MODIFIED: &str = "M";
const MARK_ADDED: &str = "A";
const MARK_DELETED: &str = "D";
const MARK_RENAMED: &str = "R";
const MARK_UNTRACKED: &str = "U";
const MARK_CONFLICTED: &str = "!";

pub(super) use drag::DraggedFile;
pub(super) use ops::FileAction;

#[derive(Default)]
pub(super) struct FileTree {
    root: Option<PathBuf>,
    group_root: Option<PathBuf>,
    expanded: HashSet<PathBuf>,
    children: HashMap<PathBuf, Vec<DirEntry>>,
    selected: Option<PathBuf>,
    pending_delete: Option<PathBuf>,
}

#[derive(Clone)]
enum TreeRow {
    Entry {
        entry: DirEntry,
        depth: usize,
        expanded: bool,
    },
    Draft {
        kind: EntryKind,
        label: String,
        depth: usize,
    },
}

struct NewEntryRow<'a> {
    dir: &'a Path,
    kind: EntryKind,
    label: String,
}

impl FileTree {
    fn rows(&self, draft: Option<&NewEntryRow<'_>>) -> Vec<TreeRow> {
        let mut rows = Vec::new();
        if let Some(root) = &self.root {
            self.push_rows(root, 0, draft, &mut rows);
        }
        rows
    }

    fn push_rows(
        &self,
        dir: &Path,
        depth: usize,
        draft: Option<&NewEntryRow<'_>>,
        rows: &mut Vec<TreeRow>,
    ) {
        if let Some(draft) = draft.filter(|draft| draft.dir == dir) {
            rows.push(TreeRow::Draft {
                kind: draft.kind,
                label: draft.label.clone(),
                depth,
            });
        }
        for entry in self.children.get(dir).into_iter().flatten() {
            let expanded = self.expanded.contains(&entry.path);
            rows.push(TreeRow::Entry {
                entry: entry.clone(),
                depth,
                expanded,
            });
            if expanded {
                self.push_rows(&entry.path, depth + 1, draft, rows);
            }
        }
    }
}

struct ChangeMarks {
    files: HashMap<PathBuf, ChangeKind>,
    folders: HashSet<PathBuf>,
}

pub(super) const fn change_mark(kind: ChangeKind) -> &'static str {
    match kind {
        ChangeKind::Modified => MARK_MODIFIED,
        ChangeKind::Added => MARK_ADDED,
        ChangeKind::Deleted => MARK_DELETED,
        ChangeKind::Renamed => MARK_RENAMED,
        ChangeKind::Untracked => MARK_UNTRACKED,
        ChangeKind::Conflicted => MARK_CONFLICTED,
    }
}

pub(super) const fn change_color(kind: ChangeKind, pal: &Palette) -> u32 {
    match kind {
        ChangeKind::Modified | ChangeKind::Renamed => pal.warning_fg,
        ChangeKind::Added | ChangeKind::Untracked => pal.success_fg,
        ChangeKind::Deleted | ChangeKind::Conflicted => pal.error_fg,
    }
}

fn followed_root(
    current: Option<&Path>,
    old_group: Option<&Path>,
    new_group: Option<&Path>,
) -> Option<PathBuf> {
    if current.is_none() || current == old_group {
        new_group.map(Path::to_path_buf)
    } else {
        current.map(Path::to_path_buf)
    }
}

impl EnvironmentShell {
    pub(super) fn sync_file_tree(&mut self, cx: &mut Context<Self>) {
        let group_root = self.active_group().map(|group| group.root.clone());
        let root = followed_root(
            self.file_tree.root.as_deref(),
            self.file_tree.group_root.as_deref(),
            group_root.as_deref(),
        );
        self.file_tree.group_root = group_root;
        if self.file_tree.root != root {
            self.show_folder(root, cx);
        }
    }

    fn show_folder(&mut self, root: Option<PathBuf>, cx: &mut Context<Self>) {
        self.file_tree = FileTree {
            root: root.clone(),
            group_root: self.file_tree.group_root.take(),
            ..FileTree::default()
        };
        if let Some(root) = root {
            Self::load_dir(root, cx);
        }
        cx.notify();
    }

    pub(super) fn file_tree_root(&self) -> Option<&Path> {
        self.file_tree.root.as_deref()
    }

    pub(super) fn file_tree_navigated(&self) -> bool {
        self.file_tree.root.is_some() && self.file_tree.root != self.file_tree.group_root
    }

    pub(super) fn navigate_file_tree(&mut self, folder: PathBuf, cx: &mut Context<Self>) {
        self.show_folder(Some(folder), cx);
    }

    pub(super) fn return_file_tree(&mut self, cx: &mut Context<Self>) {
        let root = self.file_tree.group_root.clone();
        self.show_folder(root, cx);
    }

    pub(super) fn collapse_file_tree(&mut self, cx: &mut Context<Self>) {
        self.file_tree.expanded.clear();
        cx.notify();
    }

    pub(super) fn new_entry_folder(&self) -> Option<PathBuf> {
        let root = self.file_tree.root.as_ref()?;
        let folder = self.file_tree.selected.as_ref().and_then(|selected| {
            let is_dir = self
                .file_tree
                .children
                .values()
                .flatten()
                .any(|entry| &entry.path == selected && entry.kind == EntryKind::Directory);
            if is_dir {
                Some(selected.clone())
            } else {
                selected.parent().map(Path::to_path_buf)
            }
        });
        Some(
            folder
                .filter(|folder| folder.starts_with(root))
                .unwrap_or_else(|| root.clone()),
        )
    }

    pub(super) fn reload_file_tree(&mut self, cx: &mut Context<Self>) {
        let tree = &self.file_tree;
        let dirs: HashSet<PathBuf> = tree
            .children
            .keys()
            .chain(&tree.expanded)
            .cloned()
            .collect();
        for dir in dirs {
            Self::load_dir(dir, cx);
        }
    }

    fn load_dir(dir: PathBuf, cx: &mut Context<Self>) {
        let listing = cx.background_spawn({
            let dir = dir.clone();
            async move { krad_repo::list_dir(&dir) }
        });
        cx.spawn(async move |this, cx| {
            let listing = listing.await;
            this.update(cx, |shell, cx| {
                match listing {
                    Ok(entries) => {
                        shell.file_tree.children.insert(dir, entries);
                    }
                    Err(error) => shell.push_toast(error.to_string()),
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn expand_dir(&mut self, dir: PathBuf, cx: &mut Context<Self>) {
        if self.file_tree.expanded.contains(&dir) || self.file_tree.root.as_ref() == Some(&dir) {
            return;
        }
        self.toggle_dir(dir, cx);
    }

    fn toggle_dir(&mut self, dir: PathBuf, cx: &mut Context<Self>) {
        if !self.file_tree.expanded.remove(&dir) {
            self.file_tree.expanded.insert(dir.clone());
            if !self.file_tree.children.contains_key(&dir) {
                Self::load_dir(dir, cx);
            }
        }
        cx.notify();
    }

    fn change_marks(&self) -> ChangeMarks {
        let mut marks = ChangeMarks {
            files: HashMap::new(),
            folders: HashSet::new(),
        };
        let Some(git) = self
            .file_tree
            .group_root
            .as_ref()
            .and_then(|root| self.git.get(root))
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

    fn tree_rows(&self) -> Vec<TreeRow> {
        let draft = self
            .new_entry_draft()
            .map(|(dir, kind, label)| NewEntryRow { dir, kind, label });
        self.file_tree.rows(draft.as_ref())
    }

    pub(super) fn file_tree_list(&self, cx: &mut Context<Self>) -> AnyElement {
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
                            depth,
                            expanded,
                        } => this.file_row(entry, *depth, *expanded, &marks, cx),
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
                    .on_drop(
                        cx.listener(|this: &mut Self, dragged: &drag::DraggedFile, _, cx| {
                            if let Some(root) = this.file_tree.root.clone() {
                                Self::move_file(dragged.path.clone(), root, cx);
                            }
                        }),
                    )
            })
            .child(list)
            .into_any_element()
    }

    fn on_rename_file(&mut self, _: &RenameFile, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(path) = self.file_tree.selected.clone() {
            window.focus(&self.focus, cx);
            self.start_file_rename(path, cx);
        }
    }

    fn draft_row(kind: EntryKind, label: &str, depth: usize, cx: &mut Context<Self>) -> AnyElement {
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

    fn with_entry_gestures(
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
        let dragged = drag::DraggedFile::new(entry);
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
            row.drag_over::<drag::DraggedFile>(|style, _, _, cx| style.bg(palette(cx).bg_selected))
        })
        .on_drop(
            cx.listener(move |_: &mut Self, dragged: &drag::DraggedFile, _, cx| {
                if let Some(dir) = drop_dir.clone() {
                    Self::move_file(dragged.path.clone(), dir, cx);
                }
            }),
        )
    }

    fn with_open_gestures(
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

    fn file_row(
        &self,
        entry: &DirEntry,
        depth: usize,
        expanded: bool,
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
        let label = self
            .file_rename_label(&entry.path)
            .unwrap_or_else(|| entry.name.clone());
        let name_color = match change {
            Some(kind) => change_color(kind, pal),
            None if open => pal.text_strong,
            None => pal.text_default,
        };
        let (glyph, glyph_color) = if is_dir {
            (IconName::Folder, pal.text_muted)
        } else {
            (IconName::File, pal.text_muted)
        };
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
            .child(icon(glyph, ICON_SM, glyph_color))
            .child(div().flex_1().min_w_0().truncate().child(label))
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

#[allow(clippy::cast_precision_loss)]
fn indent(depth: usize) -> f32 {
    depth as f32 * INDENT
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

    fn name(row: &TreeRow) -> (String, usize) {
        match row {
            TreeRow::Entry { entry, depth, .. } => (entry.name.clone(), *depth),
            TreeRow::Draft { label, depth, .. } => (label.clone(), *depth),
        }
    }

    fn entry(path: &str, kind: EntryKind) -> DirEntry {
        let path = PathBuf::from(path);
        DirEntry {
            name: path
                .file_name()
                .map_or_default(|name| name.to_string_lossy().into_owned()),
            path,
            kind,
        }
    }

    #[test]
    fn expanded_folders_list_their_children_right_below() {
        let mut tree = FileTree {
            root: Some(PathBuf::from("/r")),
            ..FileTree::default()
        };
        tree.children.insert(
            PathBuf::from("/r"),
            vec![
                entry("/r/src", EntryKind::Directory),
                entry("/r/README.md", EntryKind::File),
            ],
        );
        tree.children.insert(
            PathBuf::from("/r/src"),
            vec![entry("/r/src/main.rs", EntryKind::File)],
        );
        let collapsed: Vec<String> = tree.rows(None).iter().map(|row| name(row).0).collect();
        assert_eq!(collapsed, ["src", "README.md"]);
        tree.expanded.insert(PathBuf::from("/r/src"));
        let expanded: Vec<(String, usize)> = tree.rows(None).iter().map(name).collect();
        assert_eq!(
            expanded,
            [
                ("src".to_owned(), 0),
                ("main.rs".to_owned(), 1),
                ("README.md".to_owned(), 0)
            ]
        );
    }

    #[test]
    fn new_entry_draft_is_the_first_row_of_its_folder() {
        let mut tree = FileTree {
            root: Some(PathBuf::from("/r")),
            ..FileTree::default()
        };
        tree.children.insert(
            PathBuf::from("/r"),
            vec![
                entry("/r/src", EntryKind::Directory),
                entry("/r/README.md", EntryKind::File),
            ],
        );
        tree.children.insert(
            PathBuf::from("/r/src"),
            vec![entry("/r/src/main.rs", EntryKind::File)],
        );
        tree.expanded.insert(PathBuf::from("/r/src"));
        let draft = NewEntryRow {
            dir: Path::new("/r/src"),
            kind: EntryKind::File,
            label: "lib.rs".to_owned(),
        };
        let rows: Vec<(String, usize)> = tree.rows(Some(&draft)).iter().map(name).collect();
        assert_eq!(
            rows,
            [
                ("src".to_owned(), 0),
                ("lib.rs".to_owned(), 1),
                ("main.rs".to_owned(), 1),
                ("README.md".to_owned(), 0)
            ]
        );
    }

    #[test]
    fn tree_follows_the_group_only_while_it_shows_the_group_root() {
        let (api, web, home) = (Path::new("/w/api"), Path::new("/w/web"), Path::new("/u"));
        assert_eq!(
            followed_root(None, None, Some(api)),
            Some(api.to_path_buf())
        );
        assert_eq!(
            followed_root(Some(api), Some(api), Some(web)),
            Some(web.to_path_buf())
        );
        assert_eq!(
            followed_root(Some(home), Some(api), Some(web)),
            Some(home.to_path_buf())
        );
        assert_eq!(followed_root(Some(api), Some(api), None), None);
    }
}
