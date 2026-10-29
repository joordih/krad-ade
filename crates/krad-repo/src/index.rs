use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::files::EntryKind;
use crate::git::{repo_root, run_git};
use crate::RepoError;

const LS_FILES_ARGS: [&str; 5] = [
    "ls-files",
    "--cached",
    "--others",
    "--exclude-standard",
    "-z",
];
const RECORD_SEPARATOR: u8 = 0;
const HIDDEN_PREFIX: char = '.';
const MAX_WALKED_FILES: usize = 20_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexEntry {
    pub path: PathBuf,
    pub kind: EntryKind,
}

impl IndexEntry {
    fn name(&self) -> String {
        self.path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().to_lowercase())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileIndex {
    entries: Vec<IndexEntry>,
}

impl FileIndex {
    #[must_use]
    pub fn new(files: impl IntoIterator<Item = PathBuf>) -> Self {
        let files: BTreeSet<PathBuf> = files.into_iter().collect();
        let folders: BTreeSet<PathBuf> = files
            .iter()
            .flat_map(|file| file.ancestors().skip(1))
            .filter(|folder| !folder.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .collect();
        let entries = folders
            .into_iter()
            .map(|path| IndexEntry {
                path,
                kind: EntryKind::Directory,
            })
            .chain(files.into_iter().map(|path| IndexEntry {
                path,
                kind: EntryKind::File,
            }))
            .collect();
        Self { entries }
    }

    #[must_use]
    pub fn search(&self, query: &str, limit: usize) -> Vec<&IndexEntry> {
        let query = query.to_lowercase();
        let mut found: Vec<(bool, bool, usize, &IndexEntry)> = self
            .entries
            .iter()
            .filter_map(|entry| {
                let path = entry.path.to_string_lossy().to_lowercase();
                if !path.contains(&query) {
                    return None;
                }
                let name = entry.name();
                Some((
                    !name.starts_with(&query),
                    !name.contains(&query),
                    path.len(),
                    entry,
                ))
            })
            .collect();
        found.sort_by_key(|(prefix, inside, length, _)| (*prefix, *inside, *length));
        found
            .into_iter()
            .take(limit)
            .map(|(_, _, _, entry)| entry)
            .collect()
    }
}

/// # Errors
///
/// Returns [`RepoError`] when git fails or the folder cannot be read.
pub fn index_files(root: &Path) -> Result<FileIndex, RepoError> {
    if repo_root(root)?.is_none() {
        let mut files = Vec::new();
        walk(root, Path::new(""), &mut files)?;
        return Ok(FileIndex::new(files));
    }
    let output = run_git(root, &LS_FILES_ARGS)?;
    Ok(FileIndex::new(
        output
            .split(|&byte| byte == RECORD_SEPARATOR)
            .filter(|record| !record.is_empty())
            .map(|record| PathBuf::from(String::from_utf8_lossy(record).into_owned())),
    ))
}

pub(crate) fn walk(
    root: &Path,
    relative: &Path,
    files: &mut Vec<PathBuf>,
) -> Result<(), RepoError> {
    for entry in fs::read_dir(root.join(relative))? {
        if files.len() >= MAX_WALKED_FILES {
            return Ok(());
        }
        let entry = entry?;
        let name = entry.file_name();
        if name.to_string_lossy().starts_with(HIDDEN_PREFIX) {
            continue;
        }
        let path = relative.join(&name);
        if entry.file_type()?.is_dir() {
            walk(root, &path, files)?;
        } else {
            files.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> FileIndex {
        FileIndex::new(
            [
                "src/chat/composer.rs",
                "src/chat.rs",
                "docs/chat-notes.md",
                "README.md",
            ]
            .map(PathBuf::from),
        )
    }

    fn paths(found: &[&IndexEntry]) -> Vec<String> {
        found
            .iter()
            .map(|entry| entry.path.to_string_lossy().replace('\\', "/"))
            .collect()
    }

    #[test]
    fn folders_come_from_the_files_they_hold() {
        let index = index();
        let found = index.search("src", 10);
        assert_eq!(found[0].kind, EntryKind::Directory);
        assert_eq!(paths(&found[..1]), ["src"]);
    }

    #[test]
    fn names_starting_with_the_query_rank_first_then_the_shallowest() {
        let index = index();
        let found = index.search("CHAT", 10);
        assert_eq!(
            paths(&found),
            [
                "src/chat",
                "src/chat.rs",
                "docs/chat-notes.md",
                "src/chat/composer.rs"
            ]
        );
    }

    #[test]
    fn the_result_stops_at_the_limit() {
        assert_eq!(index().search("", 2).len(), 2);
        assert_eq!(index().search("missing", 10), Vec::<&IndexEntry>::new());
    }
}
