use globset::{GlobBuilder, GlobMatcher};
use ignore::{Walk, WalkBuilder};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use super::{Ran, Workspace};

const MAX_FILES: usize = 1000;
const GIT_FOLDER: &str = ".git";
const NO_FILES_TEXT: &str = "No files found";
const BAD_GLOB_TEXT: &str = "Invalid glob pattern";
const NOT_A_FOLDER_TEXT: &str = "is not a folder";

pub(super) fn walk(root: &Path) -> Walk {
    WalkBuilder::new(root)
        .hidden(false)
        .require_git(false)
        .filter_entry(|entry| entry.file_name() != GIT_FOLDER)
        .sort_by_file_path(Path::cmp)
        .build()
}

pub(super) fn glob(pattern: &str) -> Result<GlobMatcher, String> {
    GlobBuilder::new(pattern)
        .literal_separator(false)
        .build()
        .map(|glob| glob.compile_matcher())
        .map_err(|error| format!("{BAD_GLOB_TEXT}: {error}"))
}

pub(super) fn files<'a>(root: &Path, cancel: &'a AtomicBool) -> impl Iterator<Item = PathBuf> + 'a {
    walk(root)
        .take_while(move |_| !cancel.load(Ordering::SeqCst))
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
        .map(ignore::DirEntry::into_path)
}

pub(super) fn find(workspace: &Workspace, root: &Path, pattern: &str, cancel: &AtomicBool) -> Ran {
    if !root.is_dir() {
        return Ran::failed(format!("{} {NOT_A_FOLDER_TEXT}", root.display()));
    }
    let matcher = match glob(pattern) {
        Ok(matcher) => matcher,
        Err(error) => return Ran::failed(error),
    };
    let mut found = Vec::new();
    let mut more = false;
    for path in files(root, cancel) {
        let relative = path.strip_prefix(root).unwrap_or(&path);
        if !matcher.is_match(relative) {
            continue;
        }
        if found.len() == MAX_FILES {
            more = true;
            break;
        }
        found.push(workspace.shown(&path));
    }
    if found.is_empty() {
        return Ran::done(NO_FILES_TEXT.to_owned());
    }
    let mut content = found.join("\n");
    if more {
        write!(
            content,
            "\n[only the first {MAX_FILES} files are listed; narrow the pattern]"
        )
        .unwrap_or_default();
    }
    Ran::done(content)
}

#[cfg(test)]
mod tests {
    use super::super::scratch::Scratch;
    use super::*;

    #[test]
    fn files_match_the_glob_and_ignored_ones_are_skipped() {
        let scratch = Scratch::new();
        scratch.file(".gitignore", "target/\n");
        scratch.file("src/main.rs", "");
        scratch.file("src/deep/lib.rs", "");
        scratch.file("target/out.rs", "");
        scratch.file("notes.md", "");
        let workspace = Workspace {
            cwd: scratch.path().to_path_buf(),
            env: Vec::new(),
        };
        let cancel = AtomicBool::new(false);
        let ran = find(&workspace, scratch.path(), "*.rs", &cancel);
        assert_eq!(ran.content, "src/deep/lib.rs\nsrc/main.rs");
        let nested = find(&workspace, scratch.path(), "src/**/*.rs", &cancel);
        assert!(nested.content.contains("src/deep/lib.rs"));
        assert_eq!(
            find(&workspace, scratch.path(), "*.py", &cancel).content,
            NO_FILES_TEXT
        );
        assert!(find(&workspace, scratch.path(), "[", &cancel).failed);
    }
}
