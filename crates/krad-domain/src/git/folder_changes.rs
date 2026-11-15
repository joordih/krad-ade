use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::FileChange;
use crate::ChangeKind;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FolderChanges {
    pub tracked: Vec<PathBuf>,
    pub untracked: Vec<PathBuf>,
    pub crossing: Vec<FileChange>,
}

impl FolderChanges {
    #[must_use]
    pub fn len(&self) -> usize {
        self.tracked.len() + self.untracked.len() + self.crossing.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tracked.is_empty() && self.untracked.is_empty() && self.crossing.is_empty()
    }
}

#[must_use]
pub fn folder_changes(changes: &[FileChange], folder: &Path) -> FolderChanges {
    let inside = |path: &Path| path.starts_with(folder);
    let mut held = FolderChanges::default();
    for change in changes {
        let path_inside = inside(&change.path);
        let origin_inside = change.origin.as_deref().map(inside);
        if change.kind == ChangeKind::Renamed
            && origin_inside.is_some_and(|origin| origin != path_inside)
        {
            held.crossing.push(change.clone());
            continue;
        }
        let restored = path_inside.then_some(change.path.as_path()).or_else(|| {
            change
                .origin
                .as_deref()
                .filter(|_| origin_inside == Some(true))
        });
        let Some(path) = restored else {
            continue;
        };
        match change.kind {
            ChangeKind::Untracked => held.untracked.push(path.to_path_buf()),
            _ => held.tracked.push(path.to_path_buf()),
        }
    }
    held
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(path: &str, kind: ChangeKind, origin: Option<&str>) -> FileChange {
        FileChange {
            path: PathBuf::from(path),
            kind,
            origin: origin.map(PathBuf::from),
        }
    }

    #[test]
    fn a_folder_holds_the_changes_under_it_and_nothing_beside_it() {
        let changes = [
            change("src/a.rs", ChangeKind::Modified, None),
            change("src/new.rs", ChangeKind::Untracked, None),
            change("src/deep/b.rs", ChangeKind::Deleted, None),
            change("src2/c.rs", ChangeKind::Modified, None),
            change("docs/moved.md", ChangeKind::Renamed, Some("src/moved.md")),
            change("src/in.rs", ChangeKind::Renamed, Some("out.rs")),
            change("README.md", ChangeKind::Untracked, None),
        ];
        let held = folder_changes(&changes, Path::new("src"));
        assert_eq!(
            held.tracked,
            ["src/a.rs", "src/deep/b.rs"].map(PathBuf::from)
        );
        assert_eq!(held.untracked, [PathBuf::from("src/new.rs")]);
        assert_eq!(held.crossing, [changes[4].clone(), changes[5].clone()]);
        assert_eq!(held.len(), 5);
        assert_eq!(
            folder_changes(&changes, Path::new("lib")),
            FolderChanges::default()
        );
    }
}
