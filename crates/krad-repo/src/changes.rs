use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::files::read_text;
use crate::git::run_git;
use crate::status::{ChangeKind, FileChange};
use crate::RepoError;

const HEAD: &str = "HEAD";
const PATHSPEC_SEPARATOR: &str = "--";
const WHOLE_FILE_CONTEXT: &str = "--unified=1000000";
const DIFF_ARGS: [&str; 5] = [
    "diff",
    "--no-color",
    "--no-ext-diff",
    "--find-renames",
    WHOLE_FILE_CONTEXT,
];
const NUMSTAT_ARGS: [&str; 5] = ["diff", "--numstat", "-z", "--find-renames", HEAD];
const RECORD_SEPARATOR: u8 = 0;
const NUMSTAT_FIELD_SEPARATOR: char = '\t';
const HUNK_PREFIX: &str = "@@";
const ADDED_PREFIX: char = '+';
const NEWLINE: char = '\n';
const MAX_UNTRACKED_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LineCount {
    pub added: u32,
    pub removed: u32,
}

impl std::ops::Add for LineCount {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            added: self.added.saturating_add(other.added),
            removed: self.removed.saturating_add(other.removed),
        }
    }
}

/// # Errors
///
/// Returns [`RepoError::Git`] when `git diff` fails, as in a repository with
/// no commit yet.
pub fn line_counts(repo: &Path) -> Result<HashMap<PathBuf, LineCount>, RepoError> {
    run_git(repo, &NUMSTAT_ARGS).map(|stdout| parse_numstat(&stdout))
}

fn parse_numstat(output: &[u8]) -> HashMap<PathBuf, LineCount> {
    let mut counts = HashMap::new();
    let mut records = output
        .split(|&byte| byte == RECORD_SEPARATOR)
        .map(String::from_utf8_lossy);
    while let Some(record) = records.next() {
        let mut fields = record.splitn(3, NUMSTAT_FIELD_SEPARATOR);
        let (Some(added), Some(removed), Some(path)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let path = if path.is_empty() {
            records.next();
            match records.next() {
                Some(new_path) => new_path.into_owned(),
                None => continue,
            }
        } else {
            path.to_owned()
        };
        counts.insert(
            PathBuf::from(path),
            LineCount {
                added: added.parse().unwrap_or_default(),
                removed: removed.parse().unwrap_or_default(),
            },
        );
    }
    counts
}

/// # Errors
///
/// Returns [`RepoError::Git`] when `git diff` fails and
/// [`RepoError::Binary`] for an untracked binary file.
pub fn working_diff(repo: &Path, change: &FileChange) -> Result<String, RepoError> {
    if change.kind == ChangeKind::Untracked {
        let file = read_text(&repo.join(&change.path), MAX_UNTRACKED_BYTES)?;
        return Ok(added_file_diff(&file.text));
    }
    let mut args: Vec<&OsStr> = DIFF_ARGS.iter().map(OsStr::new).collect();
    args.extend([
        OsStr::new(HEAD),
        OsStr::new(PATHSPEC_SEPARATOR),
        change.path.as_os_str(),
    ]);
    let stdout = run_git(repo, &args)?;
    let diff = String::from_utf8_lossy(&stdout);
    Ok(diff
        .find(HUNK_PREFIX)
        .map_or_else(String::new, |start| diff[start..].to_owned()))
}

fn added_file_diff(text: &str) -> String {
    let count = text.lines().count();
    if count == 0 {
        return String::new();
    }
    let mut diff = format!("{HUNK_PREFIX} -0,0 +1,{count} {HUNK_PREFIX}\n");
    for line in text.lines() {
        diff.push(ADDED_PREFIX);
        diff.push_str(line);
        diff.push(NEWLINE);
    }
    diff
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numstat_reads_plain_renamed_and_binary_files() {
        let output = b"3\t1\tsrc/main.rs\x000\t2\t\x00old.rs\x00new.rs\x00-\t-\tlogo.png\x00";
        let counts = parse_numstat(output);
        assert_eq!(
            counts.get(Path::new("src/main.rs")),
            Some(&LineCount {
                added: 3,
                removed: 1
            })
        );
        assert_eq!(
            counts.get(Path::new("new.rs")),
            Some(&LineCount {
                added: 0,
                removed: 2
            })
        );
        assert_eq!(
            counts.get(Path::new("logo.png")),
            Some(&LineCount::default())
        );
        assert_eq!(counts.len(), 3);
    }

    #[test]
    fn an_untracked_file_is_one_hunk_of_added_lines() {
        assert_eq!(added_file_diff("a\nb\n"), "@@ -0,0 +1,2 @@\n+a\n+b\n");
        assert_eq!(added_file_diff(""), "");
    }
}
