use std::fs;
use std::path::{Path, PathBuf};

use crate::files::read_text;
use crate::git::repo_root;
use crate::index::walk;
use crate::process::git_command;
use crate::RepoError;

const GREP_ARGS: [&str; 9] = [
    "grep",
    "--line-number",
    "-I",
    "--ignore-case",
    "--fixed-strings",
    "-z",
    "--untracked",
    "--no-color",
    "-e",
];
const GREP_NO_MATCH: i32 = 1;
const RECORD_SEPARATOR: u8 = b'\n';
const FIELD_SEPARATOR: u8 = 0;
const MAX_SEARCHED_BYTES: u64 = 1024 * 1024;
const MAX_LINE_CHARS: usize = 240;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextMatch {
    pub path: PathBuf,
    pub line: u32,
    pub text: String,
}

/// # Errors
///
/// Returns [`RepoError`] when git fails or the folder cannot be read.
pub fn search_text(root: &Path, query: &str, limit: usize) -> Result<Vec<TextMatch>, RepoError> {
    if query.is_empty() || limit == 0 {
        return Ok(Vec::new());
    }
    if repo_root(root)?.is_some() {
        return grep(root, query, limit);
    }
    let mut files = Vec::new();
    walk(root, Path::new(""), &mut files)?;
    let needle = query.to_lowercase();
    let mut found = Vec::new();
    for relative in files {
        let path = root.join(relative);
        let small = fs::metadata(&path).is_ok_and(|meta| meta.len() <= MAX_SEARCHED_BYTES);
        let Some(file) = small
            .then(|| read_text(&path, MAX_SEARCHED_BYTES).ok())
            .flatten()
        else {
            continue;
        };
        for (index, line) in file.text.lines().enumerate() {
            if !line.to_lowercase().contains(&needle) {
                continue;
            }
            found.push(TextMatch {
                path: path.clone(),
                line: u32::try_from(index + 1).unwrap_or(u32::MAX),
                text: shown_line(line),
            });
            if found.len() >= limit {
                return Ok(found);
            }
        }
    }
    Ok(found)
}

fn grep(root: &Path, query: &str, limit: usize) -> Result<Vec<TextMatch>, RepoError> {
    let output = git_command(root).args(GREP_ARGS).arg(query).output()?;
    if output.status.code() == Some(GREP_NO_MATCH) {
        return Ok(Vec::new());
    }
    if !output.status.success() {
        return Err(RepoError::Git {
            command: GREP_ARGS[0].to_owned(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    Ok(parse_grep(root, &output.stdout, limit))
}

fn parse_grep(root: &Path, output: &[u8], limit: usize) -> Vec<TextMatch> {
    output
        .split(|&byte| byte == RECORD_SEPARATOR)
        .filter_map(|record| {
            let mut fields = record.splitn(3, |&byte| byte == FIELD_SEPARATOR);
            let path = String::from_utf8_lossy(fields.next()?);
            let line = String::from_utf8_lossy(fields.next()?).parse().ok()?;
            let text = String::from_utf8_lossy(fields.next()?);
            Some(TextMatch {
                path: root.join(path.as_ref()),
                line,
                text: shown_line(&text),
            })
        })
        .take(limit)
        .collect()
}

fn shown_line(line: &str) -> String {
    line.trim().chars().take(MAX_LINE_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grep_output_reads_into_matches_under_the_root() {
        let output = b"src/main.rs\x0012\x00    fn main() {\nREADME.md\x003\x00Main entry\n";
        let found = parse_grep(Path::new("/r"), output, 10);
        assert_eq!(
            found,
            [
                TextMatch {
                    path: PathBuf::from("/r/src/main.rs"),
                    line: 12,
                    text: "fn main() {".to_owned(),
                },
                TextMatch {
                    path: PathBuf::from("/r/README.md"),
                    line: 3,
                    text: "Main entry".to_owned(),
                },
            ]
        );
        assert_eq!(parse_grep(Path::new("/r"), output, 1).len(), 1);
    }

    #[test]
    fn a_folder_outside_a_repository_is_searched_by_reading_its_files(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let dir = std::env::temp_dir().join(format!("krad-search-{}", std::process::id()));
        fs::create_dir_all(dir.join("src"))?;
        fs::write(dir.join("src/lib.rs"), "pub fn alpha() {}\nfn Beta() {}\n")?;
        fs::write(dir.join("notes.txt"), "nothing here\n")?;
        let found = search_text(&dir, "beta", 10);
        fs::remove_dir_all(&dir)?;
        let lines: Vec<(PathBuf, u32)> = found?
            .into_iter()
            .map(|found| (found.path, found.line))
            .collect();
        assert_eq!(lines, [(dir.join("src/lib.rs"), 2)]);
        assert_eq!(search_text(Path::new("/"), "", 10)?, []);
        Ok(())
    }
}
