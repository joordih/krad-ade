mod delete_dialog;
mod drag;
mod loading;
mod ops;
mod rows;
mod upload;

use krad_domain::LanguageCatalog;
use krad_repo::{ChangeKind, DirEntry, EntryKind};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

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
const PACKAGE_SEPARATOR: &str = ".";

pub(super) use drag::DraggedFile;
pub(super) use ops::FileAction;
pub(super) use rows::{change_color, change_mark};

#[derive(Default)]
pub(super) struct FileTree {
    root: Option<PathBuf>,
    group_root: Option<PathBuf>,
    expanded: HashSet<PathBuf>,
    children: HashMap<PathBuf, Vec<DirEntry>>,
    selected: Option<PathBuf>,
    pending_delete: Option<PathBuf>,
    upload: Option<upload::UploadJob>,
}

#[derive(Clone)]
enum TreeRow {
    Entry {
        entry: DirEntry,
        label: Option<String>,
        depth: usize,
        expanded: bool,
    },
    Draft {
        kind: EntryKind,
        depth: usize,
    },
}

struct NewEntryRow<'a> {
    dir: &'a Path,
    kind: EntryKind,
}

impl FileTree {
    fn rows(&self, draft: Option<&NewEntryRow<'_>>, catalog: &LanguageCatalog) -> Vec<TreeRow> {
        let mut rows = Vec::new();
        if let Some(root) = &self.root {
            self.push_rows(root, 0, draft, catalog, &mut rows);
        }
        rows
    }

    fn only_folder_in(&self, dir: &Path) -> Option<&DirEntry> {
        match self.children.get(dir).map(Vec::as_slice) {
            Some([only]) if only.kind == EntryKind::Directory => Some(only),
            _ => None,
        }
    }

    fn compacted<'a>(
        &'a self,
        entry: &'a DirEntry,
        catalog: &LanguageCatalog,
    ) -> (&'a DirEntry, Option<String>) {
        if entry.kind != EntryKind::Directory || !catalog.in_packages(&entry.path) {
            return (entry, None);
        }
        let mut names = vec![entry.name.as_str()];
        let mut last = entry;
        while let Some(only) = self.only_folder_in(&last.path) {
            names.push(only.name.as_str());
            last = only;
        }
        let label = (names.len() > 1).then(|| names.join(PACKAGE_SEPARATOR));
        (last, label)
    }

    fn push_rows(
        &self,
        dir: &Path,
        depth: usize,
        draft: Option<&NewEntryRow<'_>>,
        catalog: &LanguageCatalog,
        rows: &mut Vec<TreeRow>,
    ) {
        if let Some(draft) = draft.filter(|draft| draft.dir == dir) {
            rows.push(TreeRow::Draft {
                kind: draft.kind,
                depth,
            });
        }
        for entry in self.children.get(dir).into_iter().flatten() {
            let (entry, label) = self.compacted(entry, catalog);
            let expanded = self.expanded.contains(&entry.path);
            rows.push(TreeRow::Entry {
                entry: entry.clone(),
                label,
                depth,
                expanded,
            });
            if expanded {
                self.push_rows(&entry.path, depth + 1, draft, catalog, rows);
            }
        }
    }
}

struct ChangeMarks {
    files: HashMap<PathBuf, ChangeKind>,
    folders: HashSet<PathBuf>,
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
#[cfg(test)]
mod tests {
    use super::*;

    const DRAFT_NAME: &str = "new entry";

    fn name(row: &TreeRow) -> (String, usize) {
        match row {
            TreeRow::Entry { entry, depth, .. } => (entry.name.clone(), *depth),
            TreeRow::Draft { depth, .. } => (DRAFT_NAME.to_owned(), *depth),
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
        let collapsed: Vec<String> = tree
            .rows(None, &LanguageCatalog::default())
            .iter()
            .map(|row| name(row).0)
            .collect();
        assert_eq!(collapsed, ["src", "README.md"]);
        tree.expanded.insert(PathBuf::from("/r/src"));
        let expanded: Vec<(String, usize)> = tree
            .rows(None, &LanguageCatalog::default())
            .iter()
            .map(name)
            .collect();
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
        };
        let rows: Vec<(String, usize)> = tree
            .rows(Some(&draft), &LanguageCatalog::default())
            .iter()
            .map(name)
            .collect();
        assert_eq!(
            rows,
            [
                ("src".to_owned(), 0),
                (DRAFT_NAME.to_owned(), 1),
                ("main.rs".to_owned(), 1),
                ("README.md".to_owned(), 0)
            ]
        );
    }

    #[test]
    fn a_package_holding_only_one_package_shares_its_row_with_it() {
        let java = krad_domain::LanguageDef {
            id: krad_domain::LanguageId::new("java"),
            name: "Java".to_owned(),
            extensions: vec!["java".to_owned()],
            file_names: Vec::new(),
            line_comment: None,
            grammar: None,
            syntax: None,
            lsp_id: None,
            server: None,
            server_runtimes: Vec::new(),
            source_folder: Some("java".to_owned()),
        };
        let catalog = LanguageCatalog::new([java]);
        let mut tree = FileTree {
            root: Some(PathBuf::from("/r/src/main/java")),
            ..FileTree::default()
        };
        tree.children.insert(
            PathBuf::from("/r/src/main/java"),
            vec![entry("/r/src/main/java/dev", EntryKind::Directory)],
        );
        tree.children.insert(
            PathBuf::from("/r/src/main/java/dev"),
            vec![entry(
                "/r/src/main/java/dev/neuralabs",
                EntryKind::Directory,
            )],
        );
        tree.children.insert(
            PathBuf::from("/r/src/main/java/dev/neuralabs"),
            vec![
                entry(
                    "/r/src/main/java/dev/neuralabs/crypto",
                    EntryKind::Directory,
                ),
                entry("/r/src/main/java/dev/neuralabs/Main.java", EntryKind::File),
            ],
        );
        tree.expanded
            .insert(PathBuf::from("/r/src/main/java/dev/neuralabs"));
        let rows: Vec<(String, usize)> = tree
            .rows(None, &catalog)
            .iter()
            .map(|row| match row {
                TreeRow::Entry {
                    entry,
                    label,
                    depth,
                    ..
                } => (label.clone().unwrap_or_else(|| entry.name.clone()), *depth),
                TreeRow::Draft { depth, .. } => (DRAFT_NAME.to_owned(), *depth),
            })
            .collect();
        assert_eq!(
            rows,
            [
                ("dev.neuralabs".to_owned(), 0),
                ("crypto".to_owned(), 1),
                ("Main.java".to_owned(), 1)
            ]
        );
        let plain: Vec<String> = tree
            .rows(None, &LanguageCatalog::default())
            .iter()
            .map(|row| name(row).0)
            .collect();
        assert_eq!(plain, ["dev"]);
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
