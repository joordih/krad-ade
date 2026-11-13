use serde::{Deserialize, Serialize};
use std::ops::Range;
use std::path::PathBuf;

const GLOB_LIST_SEPARATOR: char = ',';

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternSyntax {
    #[default]
    Literal,
    Regex,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseMatching {
    #[default]
    Insensitive,
    Sensitive,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WordMatching {
    #[default]
    Anywhere,
    WholeWord,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProjectQuery {
    pub pattern: String,
    pub syntax: PatternSyntax,
    pub case: CaseMatching,
    pub word: WordMatching,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoundLine {
    pub line: u32,
    pub text: String,
    pub matches: Vec<Range<usize>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileMatches {
    pub path: PathBuf,
    pub lines: Vec<FoundLine>,
}

impl FileMatches {
    #[must_use]
    pub fn match_count(&self) -> usize {
        self.lines.iter().map(|line| line.matches.len()).sum()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectMatches {
    pub files: Vec<FileMatches>,
    pub truncated: bool,
}

impl ProjectMatches {
    #[must_use]
    pub fn match_count(&self) -> usize {
        self.files.iter().map(FileMatches::match_count).sum()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Replacement {
    pub replaced: usize,
    pub changed_lines: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplacedFile {
    pub outcome: Replacement,
    pub undo: Option<ReplaceUndo>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplaceUndo {
    pub path: PathBuf,
    pub before: String,
    pub after: String,
}

impl ReplaceUndo {
    #[must_use]
    pub fn size(&self) -> usize {
        self.before.len() + self.after.len()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UndoOutcome {
    Restored,
    ChangedSince,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextMatch {
    pub path: PathBuf,
    pub line: u32,
    pub text: String,
}

#[must_use]
pub fn parse_globs(text: &str) -> Vec<String> {
    text.split(GLOB_LIST_SEPARATOR)
        .map(str::trim)
        .filter(|glob| !glob.is_empty())
        .map(str::to_owned)
        .collect()
}

#[must_use]
pub fn replaced_line(found: &FoundLine, replacement: &str) -> String {
    let mut line = String::with_capacity(found.text.len());
    let mut from = 0;
    for range in &found.matches {
        let Some(kept) = found.text.get(from..range.start) else {
            continue;
        };
        line.push_str(kept);
        line.push_str(replacement);
        from = range.end;
    }
    line.push_str(found.text.get(from..).unwrap_or_default());
    line
}
