use krad_domain::{line_diff, EditLine, FileEdit, LineChange};
use std::path::Path;

use super::{Occurrence, Ran, Replacement};

const EMPTY_OLD_TEXT: &str = "old_string is empty; use write to create a file";
const SAME_TEXT: &str = "old_string and new_string are the same";
const NOT_FOUND_TEXT: &str = "old_string was not found in";
const MANY_TEXT: &str = "add more context to old_string or set replace_all";
const NO_EDITS_TEXT: &str = "no edits to apply";
const EDITED_TEXT: &str = "Edited";
const EDIT_LABEL: &str = "edit";
const NOTHING_CHANGED_TEXT: &str = "nothing was changed";
const ONE_REPLACEMENT: usize = 1;
const FIRST_EDIT: usize = 1;

fn gap() -> EditLine {
    EditLine {
        change: LineChange::Gap,
        text: String::new(),
        old_line: None,
        new_line: None,
    }
}

pub(super) fn diff(replacements: &[Replacement]) -> Vec<EditLine> {
    let mut lines = Vec::new();
    for replacement in replacements {
        if !lines.is_empty() {
            lines.push(gap());
        }
        lines.extend(line_diff(&replacement.old, &replacement.new));
    }
    lines
}

fn apply(text: &str, replacement: &Replacement, shown: &str) -> Result<(String, usize), String> {
    let Replacement {
        old,
        new,
        occurrence,
    } = replacement;
    if old.is_empty() {
        return Err(EMPTY_OLD_TEXT.to_owned());
    }
    if old == new {
        return Err(SAME_TEXT.to_owned());
    }
    let found = text.matches(old.as_str()).count();
    if found == 0 {
        return Err(format!("{NOT_FOUND_TEXT} {shown}"));
    }
    match occurrence {
        Occurrence::Every => Ok((text.replace(old.as_str(), new), found)),
        Occurrence::Once if found > ONE_REPLACEMENT => Err(format!(
            "old_string found {found} times in {shown}; {MANY_TEXT}"
        )),
        Occurrence::Once => Ok((
            text.replacen(old.as_str(), new, ONE_REPLACEMENT),
            ONE_REPLACEMENT,
        )),
    }
}

fn apply_all(
    mut text: String,
    replacements: &[Replacement],
    shown: &str,
) -> Result<(String, usize), String> {
    let several = replacements.len() > ONE_REPLACEMENT;
    let mut replaced = 0;
    for (at, replacement) in replacements.iter().enumerate() {
        let (edited, count) = apply(&text, replacement, shown).map_err(|error| {
            if several {
                format!(
                    "{EDIT_LABEL} {} of {}: {error}; {NOTHING_CHANGED_TEXT}",
                    at + FIRST_EDIT,
                    replacements.len()
                )
            } else {
                error
            }
        })?;
        text = edited;
        replaced += count;
    }
    Ok((text, replaced))
}

pub(super) fn edit(path: &Path, shown: &str, replacements: &[Replacement]) -> Ran {
    if replacements.is_empty() {
        return Ran::failed(NO_EDITS_TEXT.to_owned());
    }
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => return Ran::failed(format!("{shown}: {error}")),
    };
    let (edited, replaced) = match apply_all(text, replacements, shown) {
        Ok(applied) => applied,
        Err(error) => return Ran::failed(error),
    };
    if let Err(error) = std::fs::write(path, edited) {
        return Ran::failed(format!("{shown}: {error}"));
    }
    Ran {
        content: format!("{EDITED_TEXT} {shown} ({replaced} replaced)"),
        failed: false,
        edit: Some(FileEdit {
            path: shown.to_owned(),
            lines: diff(replacements),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::super::scratch::Scratch;
    use super::*;

    fn content(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap_or_default()
    }

    fn once(old: &str, new: &str) -> Replacement {
        Replacement {
            old: old.to_owned(),
            new: new.to_owned(),
            occurrence: Occurrence::Once,
        }
    }

    fn every(old: &str, new: &str) -> Replacement {
        Replacement {
            occurrence: Occurrence::Every,
            ..once(old, new)
        }
    }

    #[test]
    fn a_unique_match_is_replaced_keeping_line_endings() {
        let scratch = Scratch::new();
        let path = scratch.file("a.txt", "one\r\ntwo\r\n");
        let ran = edit(&path, "a.txt", &[once("two", "2")]);
        assert!(!ran.failed, "{}", ran.content);
        assert_eq!(content(&path), "one\r\n2\r\n");
        assert_eq!(ran.edit.map(|edit| edit.path), Some("a.txt".to_owned()));
    }

    #[test]
    fn missing_and_repeated_text_is_refused_unless_all() {
        let scratch = Scratch::new();
        let path = scratch.file("a.txt", "x x x");
        assert!(edit(&path, "a.txt", &[once("y", "z")])
            .content
            .contains(NOT_FOUND_TEXT));
        let many = edit(&path, "a.txt", &[once("x", "z")]);
        assert!(many.failed && many.content.contains("found 3 times"));
        assert_eq!(content(&path), "x x x");
        assert!(!edit(&path, "a.txt", &[every("x", "z")]).failed);
        assert_eq!(content(&path), "z z z");
        assert!(edit(&path, "a.txt", &[once("", "z")]).failed);
        assert!(edit(&path, "a.txt", &[once("z", "z")]).failed);
    }

    #[test]
    fn several_edits_apply_in_order_or_not_at_all() {
        let scratch = Scratch::new();
        let path = scratch.file("a.rs", "fn a() {}\nfn b() {}\n");
        let ran = edit(
            &path,
            "a.rs",
            &[
                once("fn a", "fn first"),
                once("fn first() {}\nfn b", "fn first() {}\nfn second"),
            ],
        );
        assert!(!ran.failed, "{}", ran.content);
        assert_eq!(content(&path), "fn first() {}\nfn second() {}\n");
        let lines = ran.edit.map_or_default(|edit| edit.lines);
        assert_eq!(
            lines
                .iter()
                .filter(|line| line.change == LineChange::Gap)
                .count(),
            1
        );
        let refused = edit(
            &path,
            "a.rs",
            &[once("fn first", "fn one"), once("missing", "x")],
        );
        assert!(refused.failed);
        assert!(
            refused.content.starts_with("edit 2 of 2:"),
            "{}",
            refused.content
        );
        assert_eq!(content(&path), "fn first() {}\nfn second() {}\n");
    }
}
