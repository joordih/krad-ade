use gpui_kit::{ClipboardItem, Context, Window};
use krad_client::ClientError;
use krad_domain::{DirEntry, EntryKind};
use krad_protocol::{FileAsk, RepoAsk};
use std::future::Future;
use std::path::{Path, PathBuf};

use super::super::file_asks::placed;
use super::super::repo_asks::done;

use crate::components::IconName;
use crate::environment_shell::context_menu::{MenuAction, MenuEntry};
use crate::environment_shell::entry_input::EntryEdit;
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
                self.begin_entry_input(EntryEdit::Rename(path), window, cx);
            }
            FileAction::CopyPath(path) => {
                cx.write_to_clipboard(ClipboardItem::new_string(path.display().to_string()));
            }
            FileAction::CopyRelativePath(path) => {
                let relative = self.relative_to_root(&path).display().to_string();
                cx.write_to_clipboard(ClipboardItem::new_string(relative));
            }
            FileAction::Reveal(path) => {
                let reveal = self.ask_active_repo(RepoAsk::Reveal { path }, done, cx);
                Self::run_file_op(reveal, |_, (), _| {}, cx);
            }
            FileAction::Delete(path) => self.ask_to_delete(path, window, cx),
            FileAction::ShowFolder(folder) => self.navigate_file_tree(folder, cx),
        }
    }

    pub(super) fn begin_new_entry(
        &mut self,
        dir: PathBuf,
        kind: EntryKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.expand_dir(dir.clone(), cx);
        self.begin_entry_input(EntryEdit::Create { dir, kind }, window, cx);
    }

    pub(in crate::environment_shell) fn select_tree_entry(&mut self, path: PathBuf) {
        self.file_tree.selected = Some(path);
    }

    pub(in crate::environment_shell) fn relative_to_root<'a>(&self, path: &'a Path) -> &'a Path {
        self.file_tree
            .root
            .as_deref()
            .and_then(|root| path.strip_prefix(root).ok())
            .unwrap_or(path)
    }

    pub(in crate::environment_shell) fn rename_file(
        &self,
        path: PathBuf,
        name: &str,
        cx: &mut Context<Self>,
    ) {
        let ask = FileAsk::Rename {
            path: path.clone(),
            name: name.to_owned(),
        };
        Self::run_file_op(
            self.ask_active_file(ask, placed, cx),
            move |this, renamed, cx| this.relocate(&path, Some(&renamed), cx),
            cx,
        );
    }

    pub(in crate::environment_shell) fn relocate_file(
        &self,
        path: PathBuf,
        target: PathBuf,
        cx: &mut Context<Self>,
    ) {
        let ask = FileAsk::Relocate {
            path: path.clone(),
            target,
        };
        Self::run_file_op(
            self.ask_active_file(ask, placed, cx),
            move |this, moved, cx| this.relocate(&path, Some(&moved), cx),
            cx,
        );
    }

    pub(in crate::environment_shell) fn create_file_entry(
        &self,
        dir: PathBuf,
        kind: EntryKind,
        name: &str,
        cx: &mut Context<Self>,
    ) {
        let name = name.to_owned();
        let ask = match kind {
            EntryKind::File => FileAsk::CreateFile { dir, name },
            EntryKind::Directory => FileAsk::CreateDir { dir, name },
        };
        Self::run_file_op(
            self.ask_active_file(ask, placed, cx),
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
        &self,
        path: PathBuf,
        dest_dir: PathBuf,
        cx: &mut Context<Self>,
    ) {
        if dest_dir == path || path.parent() == Some(dest_dir.as_path()) {
            return;
        }
        let ask = FileAsk::Move {
            path: path.clone(),
            into: dest_dir,
        };
        Self::run_file_op(
            self.ask_active_file(ask, placed, cx),
            move |this, moved, cx| this.relocate(&path, Some(&moved), cx),
            cx,
        );
    }

    pub(super) fn confirm_file_delete(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.file_tree.pending_delete.take() else {
            return;
        };
        self.delete_file(path, cx);
    }

    pub(in crate::environment_shell) fn delete_file(&self, path: PathBuf, cx: &mut Context<Self>) {
        cx.notify();
        let ask = FileAsk::Delete { path: path.clone() };
        Self::run_file_op(
            self.ask_active_file(ask, done, cx),
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

    fn run_file_op<T: 'static>(
        op: impl Future<Output = Result<T, ClientError>> + 'static,
        done: impl FnOnce(&mut Self, T, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            let result = op.await;
            this.update(cx, |shell, cx| {
                match result {
                    Ok(value) => done(shell, value, cx),
                    Err(error) => shell.toast_failure(&error, cx),
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
