use gpui_kit::{AppContext as _, ClipboardItem, Context, Window};
use krad_repo::{
    create_dir, create_file, delete_entry, move_entry, rename_entry, reveal_in_file_manager,
    DirEntry, EntryKind, RepoError,
};
use std::path::{Path, PathBuf};

use crate::components::IconName;
use crate::environment_shell::context_menu::{MenuAction, MenuEntry};
use crate::environment_shell::EnvironmentShell;

const NEW_FILE_LABEL: &str = "New file";
const NEW_FOLDER_LABEL: &str = "New folder";
const RENAME_LABEL: &str = "Rename";
const COPY_PATH_LABEL: &str = "Copy path";
const COPY_RELATIVE_PATH_LABEL: &str = "Copy relative path";
const REVEAL_LABEL: &str = "Reveal in file manager";
const DELETE_LABEL: &str = "Delete";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) enum FileAction {
    NewFile(PathBuf),
    NewFolder(PathBuf),
    Rename(PathBuf),
    CopyPath(PathBuf),
    CopyRelativePath(PathBuf),
    Reveal(PathBuf),
    Delete(PathBuf),
    ShowFolder(PathBuf),
}

fn file_item(label: &str, glyph: Option<IconName>, action: FileAction) -> MenuEntry {
    MenuEntry::item(label, glyph, MenuAction::File(action))
}

fn relocated(path: &Path, from: &Path, to: &Path) -> Option<PathBuf> {
    let rest = path.strip_prefix(from).ok()?;
    Some(if rest.as_os_str().is_empty() {
        to.to_path_buf()
    } else {
        to.join(rest)
    })
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn folder_menu_entries(dir: &Path) -> Vec<MenuEntry> {
        vec![
            file_item(
                NEW_FILE_LABEL,
                Some(IconName::File),
                FileAction::NewFile(dir.to_path_buf()),
            ),
            file_item(
                NEW_FOLDER_LABEL,
                Some(IconName::FolderAdd),
                FileAction::NewFolder(dir.to_path_buf()),
            ),
        ]
    }

    pub(super) fn file_menu_entries(entry: &DirEntry) -> Vec<MenuEntry> {
        let path = &entry.path;
        let mut entries = Vec::new();
        if entry.kind == EntryKind::Directory {
            entries.extend(Self::folder_menu_entries(path));
            entries.push(MenuEntry::Separator);
        }
        entries.extend([
            file_item(RENAME_LABEL, None, FileAction::Rename(path.clone())),
            file_item(COPY_PATH_LABEL, None, FileAction::CopyPath(path.clone())),
            file_item(
                COPY_RELATIVE_PATH_LABEL,
                None,
                FileAction::CopyRelativePath(path.clone()),
            ),
            file_item(
                REVEAL_LABEL,
                Some(IconName::Folder),
                FileAction::Reveal(path.clone()),
            ),
            MenuEntry::Separator,
            MenuEntry::destructive(
                DELETE_LABEL,
                IconName::Close,
                MenuAction::File(FileAction::Delete(path.clone())),
            ),
        ]);
        entries
    }

    pub(in crate::environment_shell) fn run_file_action(
        &mut self,
        action: FileAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            FileAction::NewFile(dir) => self.begin_new_entry(dir, EntryKind::File, window, cx),
            FileAction::NewFolder(dir) => {
                self.begin_new_entry(dir, EntryKind::Directory, window, cx);
            }
            FileAction::Rename(path) => {
                window.focus(&self.focus, cx);
                self.start_file_rename(path, cx);
            }
            FileAction::CopyPath(path) => {
                cx.write_to_clipboard(ClipboardItem::new_string(path.display().to_string()));
            }
            FileAction::CopyRelativePath(path) => {
                let relative = self.relative_to_root(&path).display().to_string();
                cx.write_to_clipboard(ClipboardItem::new_string(relative));
            }
            FileAction::Reveal(path) => {
                Self::run_file_op(move || reveal_in_file_manager(&path), |_, (), _| {}, cx);
            }
            FileAction::Delete(path) => self.ask_to_delete(path, window, cx),
            FileAction::ShowFolder(folder) => self.navigate_file_tree(folder, cx),
        }
    }

    fn begin_new_entry(
        &mut self,
        dir: PathBuf,
        kind: EntryKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.expand_dir(dir.clone(), cx);
        window.focus(&self.focus, cx);
        self.start_new_entry(dir, kind, cx);
    }

    pub(in crate::environment_shell) fn relative_to_root<'a>(&self, path: &'a Path) -> &'a Path {
        self.file_tree
            .root
            .as_deref()
            .and_then(|root| path.strip_prefix(root).ok())
            .unwrap_or(path)
    }

    pub(in crate::environment_shell) fn rename_file(
        path: PathBuf,
        name: &str,
        cx: &mut Context<Self>,
    ) {
        let (source, name) = (path.clone(), name.to_owned());
        Self::run_file_op(
            move || rename_entry(&source, &name),
            move |this, renamed, cx| this.relocate(&path, Some(&renamed), cx),
            cx,
        );
    }

    pub(in crate::environment_shell) fn create_file_entry(
        dir: PathBuf,
        kind: EntryKind,
        name: &str,
        cx: &mut Context<Self>,
    ) {
        let name = name.to_owned();
        Self::run_file_op(
            move || match kind {
                EntryKind::File => create_file(&dir, &name),
                EntryKind::Directory => create_dir(&dir, &name),
            },
            move |this, created, cx| {
                this.file_tree.selected = Some(created.clone());
                if kind == EntryKind::File {
                    this.open_file(created, cx);
                }
                this.refresh_file_tree(cx);
            },
            cx,
        );
    }

    pub(in crate::environment_shell) fn move_file(
        path: PathBuf,
        dest_dir: PathBuf,
        cx: &mut Context<Self>,
    ) {
        if dest_dir == path || path.parent() == Some(dest_dir.as_path()) {
            return;
        }
        let source = path.clone();
        Self::run_file_op(
            move || move_entry(&source, &dest_dir),
            move |this, moved, cx| this.relocate(&path, Some(&moved), cx),
            cx,
        );
    }

    pub(super) fn confirm_file_delete(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.file_tree.pending_delete.take() else {
            return;
        };
        Self::delete_file(path, cx);
    }

    pub(super) fn delete_file(path: PathBuf, cx: &mut Context<Self>) {
        cx.notify();
        let target = path.clone();
        Self::run_file_op(
            move || delete_entry(&target),
            move |this, (), cx| this.relocate(&path, None, cx),
            cx,
        );
    }

    fn relocate(&mut self, from: &Path, to: Option<&Path>, cx: &mut Context<Self>) {
        let follow = |path: PathBuf| {
            if path.starts_with(from) {
                to.and_then(|to| relocated(&path, from, to))
            } else {
                Some(path)
            }
        };
        let tree = &mut self.file_tree;
        tree.expanded = tree.expanded.drain().filter_map(follow).collect();
        tree.children.retain(|dir, _| !dir.starts_with(from));
        tree.selected = tree.selected.take().and_then(follow);
        let viewed = self
            .viewer
            .as_ref()
            .map(|viewer| viewer.path.clone())
            .filter(|viewed| viewed.starts_with(from));
        if let Some(viewed) = viewed {
            match follow(viewed) {
                Some(moved) => self.open_file(moved, cx),
                None => self.close_viewer(cx),
            }
        }
        self.relocate_notes(from, to, cx);
        self.refresh_file_tree(cx);
    }

    fn refresh_file_tree(&mut self, cx: &mut Context<Self>) {
        self.reload_file_tree(cx);
        self.refresh_git(cx);
    }

    fn run_file_op<T: Send + 'static>(
        op: impl FnOnce() -> Result<T, RepoError> + Send + 'static,
        done: impl FnOnce(&mut Self, T, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let task = cx.background_spawn(async move { op() });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |shell, cx| {
                match result {
                    Ok(value) => done(shell, value, cx),
                    Err(error) => shell.push_toast(error.to_string()),
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relocated_follows_the_entry_and_what_it_holds() {
        let (from, to) = (Path::new("/r/a"), Path::new("/r/b/a"));
        assert_eq!(relocated(from, from, to), Some(to.to_path_buf()));
        assert_eq!(
            relocated(Path::new("/r/a/src/main.rs"), from, to),
            Some(PathBuf::from("/r/b/a/src/main.rs"))
        );
        assert_eq!(relocated(Path::new("/r/ab"), from, to), None);
    }
}
