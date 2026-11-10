use krad_host::HostLink;
use std::collections::HashMap;
use std::path::PathBuf;

use super::{FileMatches, FoundLine};
use crate::RepoError;

const LINE_END: &str = "\n";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Replacement {
    pub replaced: usize,
    pub changed_lines: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplacedFile {
    pub outcome: Replacement,
    pub undo: Option<ReplaceUndo>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplaceUndo {
    pub path: PathBuf,
    before: String,
    after: String,
}

impl ReplaceUndo {
    #[must_use]
    pub fn size(&self) -> usize {
        self.before.len() + self.after.len()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UndoOutcome {
    Restored,
    ChangedSince,
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

/// # Errors
///
/// Returns [`RepoError`] when the file cannot be read or written, or is not UTF-8 text.
pub fn replace_in_file(
    host: &HostLink,
    file: &FileMatches,
    replacement: &str,
) -> Result<ReplacedFile, RepoError> {
    let bytes = host.read_all(&file.path)?;
    let text = String::from_utf8(bytes).map_err(|_| RepoError::Binary)?;
    let (replaced_text, outcome) = replace_lines(&text, &file.lines, replacement);
    if outcome.replaced == 0 {
        return Ok(ReplacedFile {
            outcome,
            undo: None,
        });
    }
    host.write(&file.path, replaced_text.as_bytes())?;
    Ok(ReplacedFile {
        outcome,
        undo: Some(ReplaceUndo {
            path: file.path.clone(),
            before: text,
            after: replaced_text,
        }),
    })
}

/// # Errors
///
/// Returns [`RepoError`] when the file cannot be read or written. A file
/// edited since the replacement is left as it is and reported as
/// [`UndoOutcome::ChangedSince`].
pub fn undo_replace(host: &HostLink, undo: &ReplaceUndo) -> Result<UndoOutcome, RepoError> {
    let current = host.read_all(&undo.path)?;
    if current != undo.after.as_bytes() {
        return Ok(UndoOutcome::ChangedSince);
    }
    host.write(&undo.path, undo.before.as_bytes())?;
    Ok(UndoOutcome::Restored)
}

fn replace_lines(text: &str, lines: &[FoundLine], replacement: &str) -> (String, Replacement) {
    let wanted: HashMap<usize, &FoundLine> = lines
        .iter()
        .filter_map(|found| Some((usize::try_from(found.line).ok()?.checked_sub(1)?, found)))
        .collect();
    let mut outcome = Replacement::default();
    let mut replaced = String::with_capacity(text.len());
    for (index, line) in text.split_inclusive(LINE_END).enumerate() {
        let (content, ending) = line
            .strip_suffix(LINE_END)
            .map_or((line, ""), |content| (content, LINE_END));
        match wanted.get(&index) {
            Some(found) if found.text == content => {
                replaced.push_str(&replaced_line(found, replacement));
                replaced.push_str(ending);
                outcome.replaced += found.matches.len();
            }
            Some(_) => {
                outcome.changed_lines += 1;
                replaced.push_str(line);
            }
            None => replaced.push_str(line),
        }
    }
    (replaced, outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    use std::fs;
    use std::ops::Range;
    use std::path::PathBuf;

    fn spans(pairs: &[(usize, usize)]) -> Vec<Range<usize>> {
        pairs.iter().map(|&(start, end)| start..end).collect()
    }

    fn found(line: u32, text: &str, matches: Vec<Range<usize>>) -> FoundLine {
        FoundLine {
            line,
            text: text.to_owned(),
            matches,
        }
    }

    #[test]
    fn the_preview_swaps_every_match_on_the_line() {
        let line = found(1, "foo bar foo", vec![0..3, 8..11]);
        assert_eq!(replaced_line(&line, "baz"), "baz bar baz");
        assert_eq!(replaced_line(&line, ""), " bar ");
        assert_eq!(replaced_line(&found(1, "plain", Vec::new()), "x"), "plain");
    }

    #[test]
    fn only_lines_still_as_found_are_replaced_keeping_line_endings() {
        let text = "foo\r\nkeep\nfoo changed\nlast foo";
        let lines = [
            found(1, "foo\r", spans(&[(0, 3)])),
            found(3, "foo", spans(&[(0, 3)])),
            found(4, "last foo", spans(&[(5, 8)])),
        ];
        let (replaced, outcome) = replace_lines(text, &lines, "bar");
        assert_eq!(replaced, "bar\r\nkeep\nfoo changed\nlast bar");
        assert_eq!(
            outcome,
            Replacement {
                replaced: 2,
                changed_lines: 1
            }
        );
    }

    #[test]
    fn replacing_in_a_file_writes_it_back() -> Result<(), Box<dyn Error>> {
        let dir = std::env::temp_dir().join(format!("krad-replace-{}", std::process::id()));
        fs::create_dir_all(&dir)?;
        let path = dir.join("a.txt");
        fs::write(&path, "alpha beta\nalpha\n")?;
        let file = FileMatches {
            path: path.clone(),
            lines: vec![
                found(1, "alpha beta", spans(&[(0, 5)])),
                found(2, "alpha", spans(&[(0, 5)])),
            ],
        };
        let replaced = replace_in_file(&HostLink::Local, &file, "omega");
        let written = fs::read_to_string(&path);
        fs::write(&path, [0xff, 0xfe])?;
        let binary = replace_in_file(&HostLink::Local, &file, "omega");
        fs::remove_dir_all(&dir)?;
        assert_eq!(replaced?.outcome.replaced, 2);
        assert_eq!(written?, "omega beta\nomega\n");
        assert!(matches!(binary, Err(RepoError::Binary)));
        let missing = FileMatches {
            path: PathBuf::from("/nonexistent-krad/a.txt"),
            lines: Vec::new(),
        };
        assert!(replace_in_file(&HostLink::Local, &missing, "x").is_err());
        Ok(())
    }

    #[test]
    fn undoing_restores_the_file_unless_it_changed_since() -> Result<(), Box<dyn Error>> {
        let dir = std::env::temp_dir().join(format!("krad-replace-undo-{}", std::process::id()));
        fs::create_dir_all(&dir)?;
        let path = dir.join("a.txt");
        fs::write(&path, "alpha\n")?;
        let file = FileMatches {
            path: path.clone(),
            lines: vec![found(1, "alpha", spans(&[(0, 5)]))],
        };
        let first = replace_in_file(&HostLink::Local, &file, "omega")?.undo;
        let restored = first
            .as_ref()
            .map(|undo| undo_replace(&HostLink::Local, undo))
            .transpose()?;
        let back = fs::read_to_string(&path)?;
        let second = replace_in_file(&HostLink::Local, &file, "omega")?.undo;
        fs::write(&path, "edited\n")?;
        let refused = second
            .as_ref()
            .map(|undo| undo_replace(&HostLink::Local, undo))
            .transpose()?;
        let kept = fs::read_to_string(&path)?;
        fs::remove_dir_all(&dir)?;
        assert_eq!(restored, Some(UndoOutcome::Restored));
        assert_eq!(back, "alpha\n");
        assert_eq!(refused, Some(UndoOutcome::ChangedSince));
        assert_eq!(kept, "edited\n");
        Ok(())
    }
}
