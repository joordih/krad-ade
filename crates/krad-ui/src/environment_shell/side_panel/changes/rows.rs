use krad_domain::ChangesLayout;
use krad_repo::FileChange;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use super::super::super::git_actions::ChangeSide;

const FOLDER_SEPARATOR: &str = "/";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ChangeRow {
    Section {
        side: ChangeSide,
        count: usize,
        collapsed: bool,
    },
    Folder {
        side: ChangeSide,
        path: PathBuf,
        label: String,
        depth: usize,
        collapsed: bool,
    },
    File {
        side: ChangeSide,
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
        side: ChangeSide,
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
                side,
                path: path.clone(),
                label,
                depth,
                collapsed: shut,
            });
            if !shut {
                node.push_rows(side, &path, depth + 1, expanded, rows);
            }
        }
        let mut files = self.files.clone();
        files.sort();
        rows.extend(files.into_iter().map(|(_, change)| ChangeRow::File {
            side,
            change,
            depth,
        }));
    }
}

pub(super) fn side_rows(
    changes: &[FileChange],
    side: ChangeSide,
    layout: ChangesLayout,
    expanded: &HashSet<PathBuf>,
) -> Vec<ChangeRow> {
    match layout {
        ChangesLayout::List => {
            let mut order: Vec<usize> = (0..changes.len()).collect();
            order.sort_by(|a, b| changes[*a].path.cmp(&changes[*b].path));
            order
                .into_iter()
                .map(|change| ChangeRow::File {
                    side,
                    change,
                    depth: 0,
                })
                .collect()
        }
        ChangesLayout::Tree => {
            let mut root = FolderNode::default();
            for (index, change) in changes.iter().enumerate() {
                root.insert(&change.path, index);
            }
            let mut rows = Vec::new();
            root.push_rows(side, Path::new(""), 0, expanded, &mut rows);
            rows
        }
    }
}

pub(super) struct Sides<'a> {
    pub(super) staged: &'a [FileChange],
    pub(super) unstaged: &'a [FileChange],
}

pub(super) fn change_rows(
    sides: &Sides<'_>,
    layout: ChangesLayout,
    expanded: &HashSet<PathBuf>,
    collapsed: &HashSet<ChangeSide>,
) -> Vec<ChangeRow> {
    let mut rows = Vec::new();
    for (side, changes) in [
        (ChangeSide::Staged, sides.staged),
        (ChangeSide::Unstaged, sides.unstaged),
    ] {
        if changes.is_empty() {
            continue;
        }
        let shut = collapsed.contains(&side);
        rows.push(ChangeRow::Section {
            side,
            count: changes.len(),
            collapsed: shut,
        });
        if !shut {
            rows.extend(side_rows(changes, side, layout, expanded));
        }
    }
    rows
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
                origin: None,
            })
            .collect()
    }

    fn shown(rows: &[ChangeRow], changes: &[FileChange]) -> Vec<(String, usize)> {
        rows.iter()
            .map(|row| match row {
                ChangeRow::Section { side, count, .. } => (format!("{side:?} {count}"), 0),
                ChangeRow::Folder { label, depth, .. } => (format!("{label}/"), *depth),
                ChangeRow::File { change, depth, .. } => {
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
        let side = ChangeSide::Unstaged;
        let folded = side_rows(&changes, side, ChangesLayout::Tree, &HashSet::new());
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
        let rows = side_rows(&changes, side, ChangesLayout::Tree, &expanded);
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
        let side = ChangeSide::Unstaged;
        let expanded: HashSet<PathBuf> = [PathBuf::from("docs")].into();
        let rows = side_rows(&changes, side, ChangesLayout::Tree, &expanded);
        assert_eq!(
            shown(&rows, &changes),
            [
                ("docs/".to_owned(), 0),
                ("docs/a.md".to_owned(), 1),
                ("src/".to_owned(), 0),
            ]
        );
        let list = side_rows(&changes, side, ChangesLayout::List, &HashSet::new());
        assert_eq!(
            shown(&list, &changes),
            [
                ("docs/a.md".to_owned(), 0),
                ("src/a.rs".to_owned(), 0),
                ("src/b.rs".to_owned(), 0),
            ]
        );
    }

    #[test]
    fn staged_changes_come_first_and_a_folded_side_hides_its_files() {
        let staged = changes(&["a.rs"]);
        let unstaged = changes(&["b.rs", "c.rs"]);
        let sides = Sides {
            staged: &staged,
            unstaged: &unstaged,
        };
        let rows = change_rows(
            &sides,
            ChangesLayout::List,
            &HashSet::new(),
            &HashSet::new(),
        );
        let sections: Vec<_> = rows
            .iter()
            .filter_map(|row| match row {
                ChangeRow::Section { side, count, .. } => Some((*side, *count)),
                _ => None,
            })
            .collect();
        assert_eq!(
            sections,
            [(ChangeSide::Staged, 1), (ChangeSide::Unstaged, 2)]
        );
        assert_eq!(rows.len(), 5);
        let folded: HashSet<ChangeSide> = [ChangeSide::Unstaged].into();
        let rows = change_rows(&sides, ChangesLayout::List, &HashSet::new(), &folded);
        assert_eq!(rows.len(), 3);
        let only_unstaged = Sides {
            staged: &[],
            unstaged: &unstaged,
        };
        let rows = change_rows(
            &only_unstaged,
            ChangesLayout::List,
            &HashSet::new(),
            &folded,
        );
        assert_eq!(
            rows,
            [ChangeRow::Section {
                side: ChangeSide::Unstaged,
                count: 2,
                collapsed: true,
            }]
        );
    }
}
