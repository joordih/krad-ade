use gpui_kit::{AppContext as _, Context};
use krad_repo::EntryKind;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::super::EnvironmentShell;
use super::{followed_root, FileTree};

impl EnvironmentShell {
    pub(in crate::environment_shell) fn sync_file_tree(&mut self, cx: &mut Context<Self>) {
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

    pub(super) fn show_folder(&mut self, root: Option<PathBuf>, cx: &mut Context<Self>) {
        self.file_tree = FileTree {
            root: root.clone(),
            group_root: self.file_tree.group_root.take(),
            ..FileTree::default()
        };
        if let Some(root) = root {
            self.load_dir(root, cx);
        }
        cx.notify();
    }

    pub(in crate::environment_shell) fn file_tree_root(&self) -> Option<&Path> {
        self.file_tree.root.as_deref()
    }

    pub(in crate::environment_shell) fn file_tree_navigated(&self) -> bool {
        self.file_tree.root.is_some() && self.file_tree.root != self.file_tree.group_root
    }

    pub(in crate::environment_shell) fn navigate_file_tree(
        &mut self,
        folder: PathBuf,
        cx: &mut Context<Self>,
    ) {
        self.show_folder(Some(folder), cx);
    }

    pub(in crate::environment_shell) fn return_file_tree(&mut self, cx: &mut Context<Self>) {
        let root = self.file_tree.group_root.clone();
        self.show_folder(root, cx);
    }

    pub(in crate::environment_shell) fn collapse_file_tree(&mut self, cx: &mut Context<Self>) {
        self.file_tree.expanded.clear();
        cx.notify();
    }

    pub(in crate::environment_shell) fn new_entry_folder(&self) -> Option<PathBuf> {
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

    pub(in crate::environment_shell) fn reload_file_tree(&mut self, cx: &mut Context<Self>) {
        let tree = &self.file_tree;
        let dirs: HashSet<PathBuf> = tree
            .children
            .keys()
            .chain(&tree.expanded)
            .cloned()
            .collect();
        for dir in dirs {
            self.load_dir(dir, cx);
        }
    }

    pub(in crate::environment_shell) fn reload_file_dirs(
        &mut self,
        dirs: &HashSet<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        let loaded: Vec<PathBuf> = dirs
            .iter()
            .filter(|dir| self.file_tree.children.contains_key(*dir))
            .cloned()
            .collect();
        for dir in loaded {
            self.load_dir(dir, cx);
        }
    }

    pub(super) fn load_dir(&self, dir: PathBuf, cx: &mut Context<Self>) {
        let link = self.active_link();
        let listing = cx.background_spawn({
            let dir = dir.clone();
            async move { krad_repo::list_dir(&link, &dir) }
        });
        cx.spawn(async move |this, cx| {
            let listing = listing.await;
            this.update(cx, |shell, cx| {
                match listing {
                    Ok(entries) => {
                        shell.file_tree.children.insert(dir.clone(), entries);
                        shell.load_package_chain(&dir, cx);
                    }
                    Err(error) => shell.toast_failure(&error),
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn load_package_chain(&mut self, dir: &Path, cx: &mut Context<Self>) {
        let Some(only) = self
            .file_tree
            .only_folder_in(dir)
            .map(|only| only.path.clone())
        else {
            return;
        };
        if !self.language_catalog().in_packages(&only) {
            return;
        }
        if self.language_catalog().in_packages(dir) && self.file_tree.expanded.remove(dir) {
            self.file_tree.expanded.insert(only.clone());
        }
        if !self.file_tree.children.contains_key(&only) {
            self.load_dir(only, cx);
        }
    }

    pub(super) fn expand_dir(&mut self, dir: PathBuf, cx: &mut Context<Self>) {
        if self.file_tree.expanded.contains(&dir) || self.file_tree.root.as_ref() == Some(&dir) {
            return;
        }
        self.toggle_dir(dir, cx);
    }

    pub(super) fn toggle_dir(&mut self, dir: PathBuf, cx: &mut Context<Self>) {
        if !self.file_tree.expanded.remove(&dir) {
            self.file_tree.expanded.insert(dir.clone());
            if !self.file_tree.children.contains_key(&dir) {
                self.load_dir(dir, cx);
            }
        }
        cx.notify();
    }
}
