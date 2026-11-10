use krad_host::HostLink;
use std::ffi::OsString;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::thread;

use crate::git::repo_root;
use crate::process::git_output;
use crate::RepoError;

mod output;
mod replace;

pub use replace::{
    replace_in_file, replaced_line, undo_replace, ReplaceUndo, ReplacedFile, Replacement,
    UndoOutcome,
};

const GREP_COMMAND: &str = "grep";
const GREP_COMMON_ARGS: [&str; 4] = ["--line-number", "-z", "-I", "--no-color"];
const IN_REPOSITORY_ARG: &str = "--untracked";
const OUTSIDE_REPOSITORY_ARG: &str = "--no-index";
const IGNORE_CASE_ARG: &str = "--ignore-case";
const WHOLE_WORD_ARG: &str = "--word-regexp";
const LITERAL_ARG: &str = "--fixed-strings";
const REGEX_ARG: &str = "--extended-regexp";
const ONLY_MATCHES_ARGS: [&str; 2] = ["--only-matching", "--column"];
const PATTERN_ARG: &str = "-e";
const PATHSPEC_SEPARATOR: &str = "--";
const GREP_NO_MATCH: i32 = 1;
const SEARCH_STOPPED_TEXT: &str = "the search stopped unexpectedly";
const GLOB_LIST_SEPARATOR: char = ',';
const GLOB_MAGIC: &str = ":(glob)";
const EXCLUDE_GLOB_MAGIC: &str = ":(exclude,glob)";
const ANY_FOLDER: &str = "**/";
const ANYTHING_INSIDE: &str = "/**";
const FOLDER_SEPARATOR: char = '/';
const CURRENT_FOLDER: &str = "./";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PatternSyntax {
    #[default]
    Literal,
    Regex,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CaseMatching {
    #[default]
    Insensitive,
    Sensitive,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WordMatching {
    #[default]
    Anywhere,
    WholeWord,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ProjectQuery {
    pub pattern: String,
    pub syntax: PatternSyntax,
    pub case: CaseMatching,
    pub word: WordMatching,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoundLine {
    pub line: u32,
    pub text: String,
    pub matches: Vec<Range<usize>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
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

#[must_use]
pub fn parse_globs(text: &str) -> Vec<String> {
    text.split(GLOB_LIST_SEPARATOR)
        .map(str::trim)
        .filter(|glob| !glob.is_empty())
        .map(str::to_owned)
        .collect()
}

/// # Errors
///
/// Returns [`RepoError`] when `git grep` fails, for example on an invalid regular expression.
pub fn search_project(
    host: &HostLink,
    root: &Path,
    query: &ProjectQuery,
    line_limit: usize,
) -> Result<ProjectMatches, RepoError> {
    if query.pattern.is_empty() || line_limit == 0 {
        return Ok(ProjectMatches::default());
    }
    let in_repository = repo_root(host, root)?.is_some();
    let lines_args = grep_args(query, in_repository, false);
    let matches_args = grep_args(query, in_repository, true);
    let (lines, matches) = thread::scope(|scope| {
        let lines = scope.spawn(|| grep(host, root, &lines_args));
        let matches = grep(host, root, &matches_args);
        (lines.join(), matches)
    });
    let lines = lines.map_err(|_| RepoError::Git {
        command: GREP_COMMAND.to_owned(),
        message: SEARCH_STOPPED_TEXT.to_owned(),
    })??;
    Ok(output::combine(root, &lines, &matches?, line_limit))
}

fn grep(host: &HostLink, root: &Path, args: &[OsString]) -> Result<Vec<u8>, RepoError> {
    let output = git_output(host, root, args)?;
    if output.status.code() == Some(GREP_NO_MATCH) {
        return Ok(Vec::new());
    }
    if !output.status.success() {
        return Err(RepoError::Git {
            command: GREP_COMMAND.to_owned(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    Ok(output.stdout)
}

fn grep_args(query: &ProjectQuery, in_repository: bool, only_matches: bool) -> Vec<OsString> {
    let place = if in_repository {
        IN_REPOSITORY_ARG
    } else {
        OUTSIDE_REPOSITORY_ARG
    };
    let syntax = match query.syntax {
        PatternSyntax::Literal => LITERAL_ARG,
        PatternSyntax::Regex => REGEX_ARG,
    };
    let mut flags = vec![GREP_COMMAND];
    flags.extend(GREP_COMMON_ARGS);
    flags.extend([place, syntax]);
    if query.case == CaseMatching::Insensitive {
        flags.push(IGNORE_CASE_ARG);
    }
    if query.word == WordMatching::WholeWord {
        flags.push(WHOLE_WORD_ARG);
    }
    if only_matches {
        flags.extend(ONLY_MATCHES_ARGS);
    }
    flags.push(PATTERN_ARG);
    flags
        .into_iter()
        .map(OsString::from)
        .chain([OsString::from(&query.pattern), PATHSPEC_SEPARATOR.into()])
        .chain(pathspecs(&query.include, GLOB_MAGIC))
        .chain(pathspecs(&query.exclude, EXCLUDE_GLOB_MAGIC))
        .collect()
}

fn pathspecs<'a>(globs: &'a [String], magic: &'a str) -> impl Iterator<Item = OsString> + 'a {
    globs.iter().flat_map(move |glob| {
        let glob = glob.trim_start_matches(CURRENT_FOLDER);
        let anywhere = if glob.contains(FOLDER_SEPARATOR) {
            glob.to_owned()
        } else {
            format!("{ANY_FOLDER}{glob}")
        };
        let inside = format!("{anywhere}{ANYTHING_INSIDE}");
        [anywhere, inside].map(|spec| OsString::from(format!("{magic}{spec}")))
    })
}

#[cfg(test)]
mod tests;
