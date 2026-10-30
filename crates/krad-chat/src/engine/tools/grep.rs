use grep_regex::RegexMatcherBuilder;
use grep_searcher::sinks::Lossy;
use grep_searcher::{BinaryDetection, SearcherBuilder};
use std::fmt::Write as _;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use super::find::{files, glob};
use super::{Ran, Workspace};

const MAX_MATCHES: usize = 500;
const MAX_LINE_CHARS: usize = 300;
const LINE_CUT_MARK: &str = "…";
const NO_MATCHES_TEXT: &str = "No matches";
const BAD_PATTERN_TEXT: &str = "Invalid regular expression";
const NUL: u8 = 0;

pub(super) struct Query<'a> {
    pub(super) pattern: &'a str,
    pub(super) glob: Option<&'a str>,
    pub(super) ignore_case: bool,
}

fn cut(line: &str) -> String {
    let line = line.trim_end();
    match line.char_indices().nth(MAX_LINE_CHARS) {
        Some((end, _)) => format!("{}{LINE_CUT_MARK}", &line[..end]),
        None => line.to_owned(),
    }
}

pub(super) fn grep(workspace: &Workspace, root: &Path, query: &Query, cancel: &AtomicBool) -> Ran {
    let matcher = match RegexMatcherBuilder::new()
        .case_insensitive(query.ignore_case)
        .build(query.pattern)
    {
        Ok(matcher) => matcher,
        Err(error) => return Ran::failed(format!("{BAD_PATTERN_TEXT}: {error}")),
    };
    let filter = match query.glob.map(glob).transpose() {
        Ok(filter) => filter,
        Err(error) => return Ran::failed(error),
    };
    let mut searcher = SearcherBuilder::new()
        .line_number(true)
        .binary_detection(BinaryDetection::quit(NUL))
        .build();
    let mut found: Vec<String> = Vec::new();
    let mut more = false;
    for path in files(root, cancel) {
        let relative = path.strip_prefix(root).unwrap_or(&path);
        let name = path.file_name().map_or(relative, Path::new);
        if filter
            .as_ref()
            .is_some_and(|filter| !filter.is_match(relative) && !filter.is_match(name))
        {
            continue;
        }
        let place = workspace.shown(&path);
        let outcome = searcher.search_path(
            &matcher,
            &path,
            Lossy(|number, line| {
                if found.len() == MAX_MATCHES {
                    more = true;
                    return Ok(false);
                }
                found.push(format!("{place}:{number}:{}", cut(line)));
                Ok(true)
            }),
        );
        if outcome.is_err() {
            continue;
        }
        if more {
            break;
        }
    }
    if found.is_empty() {
        return Ran::done(NO_MATCHES_TEXT.to_owned());
    }
    let mut content = found.join("\n");
    if more {
        write!(
            content,
            "\n[only the first {MAX_MATCHES} matches are listed; narrow the search]"
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
    fn matches_come_with_path_and_line_honouring_case_and_glob() {
        let scratch = Scratch::new();
        scratch.file("src/a.rs", "fn main() {}\n// TODO later\n");
        scratch.file("notes.md", "todo: write docs\n");
        let workspace = Workspace {
            cwd: scratch.path().to_path_buf(),
            env: Vec::new(),
        };
        let cancel = AtomicBool::new(false);
        let query = |pattern, glob, ignore_case| Query {
            pattern,
            glob,
            ignore_case,
        };
        let exact = grep(
            &workspace,
            scratch.path(),
            &query("TODO", None, false),
            &cancel,
        );
        assert_eq!(exact.content, "src/a.rs:2:// TODO later");
        let any_case = grep(
            &workspace,
            scratch.path(),
            &query("todo", None, true),
            &cancel,
        );
        assert_eq!(any_case.content.lines().count(), 2);
        let only_rust = grep(
            &workspace,
            scratch.path(),
            &query("todo", Some("*.rs"), true),
            &cancel,
        );
        assert_eq!(only_rust.content, "src/a.rs:2:// TODO later");
        let none = grep(
            &workspace,
            scratch.path(),
            &query("absent", None, false),
            &cancel,
        );
        assert_eq!(none.content, NO_MATCHES_TEXT);
        assert!(
            grep(
                &workspace,
                scratch.path(),
                &query("(", None, false),
                &cancel
            )
            .failed
        );
    }
}
