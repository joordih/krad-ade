use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};

use super::{FileMatches, FoundLine, ProjectMatches};

const RECORD_SEPARATOR: u8 = b'\n';
const FIELD_SEPARATOR: u8 = 0;
const LINE_FIELDS: usize = 3;
const MATCH_FIELDS: usize = 4;

type LineKey = (PathBuf, u32);

pub(super) fn combine(
    root: &Path,
    lines: &[u8],
    matches: &[u8],
    line_limit: usize,
) -> ProjectMatches {
    let spans = match_spans(matches);
    let mut found = ProjectMatches::default();
    let mut kept = 0;
    for (path, line, text) in records(lines, LINE_FIELDS).filter_map(|fields| line_record(&fields))
    {
        let ranges = spans
            .get(&(path.clone(), line))
            .map_or_default(|spans| valid_ranges(&text, spans));
        if ranges.is_empty() {
            continue;
        }
        if kept == line_limit {
            found.truncated = true;
            break;
        }
        kept += 1;
        let found_line = FoundLine {
            line,
            text,
            matches: ranges,
        };
        let path = root.join(path);
        match found.files.last_mut() {
            Some(file) if file.path == path => file.lines.push(found_line),
            _ => found.files.push(FileMatches {
                path,
                lines: vec![found_line],
            }),
        }
    }
    found
}

fn records(output: &[u8], fields: usize) -> impl Iterator<Item = Vec<&[u8]>> {
    output
        .split(|&byte| byte == RECORD_SEPARATOR)
        .map(move |record| {
            record
                .splitn(fields, |&byte| byte == FIELD_SEPARATOR)
                .collect()
        })
}

fn line_record(fields: &[&[u8]]) -> Option<(PathBuf, u32, String)> {
    let [path, line, text] = fields else {
        return None;
    };
    Some((
        PathBuf::from(String::from_utf8_lossy(path).as_ref()),
        number(line)?,
        String::from_utf8_lossy(text).into_owned(),
    ))
}

fn match_spans(output: &[u8]) -> HashMap<LineKey, Vec<(usize, String)>> {
    let mut spans: HashMap<LineKey, Vec<(usize, String)>> = HashMap::new();
    for fields in records(output, MATCH_FIELDS) {
        let [path, line, column, matched] = fields.as_slice() else {
            continue;
        };
        let (Some(line), Some(column)) = (number(line), number::<usize>(column)) else {
            continue;
        };
        let key = (PathBuf::from(String::from_utf8_lossy(path).as_ref()), line);
        let start = column.saturating_sub(1);
        spans
            .entry(key)
            .or_default()
            .push((start, String::from_utf8_lossy(matched).into_owned()));
    }
    spans
}

fn valid_ranges(text: &str, spans: &[(usize, String)]) -> Vec<Range<usize>> {
    let mut ranges: Vec<Range<usize>> = spans
        .iter()
        .filter(|(_, matched)| !matched.is_empty())
        .filter_map(|(start, matched)| {
            let range = *start..start + matched.len();
            (text.get(range.clone()) == Some(matched.as_str())).then_some(range)
        })
        .collect();
    ranges.sort_by_key(|range| range.start);
    ranges.dedup_by(|later, earlier| later.start < earlier.end);
    ranges
}

fn number<T: std::str::FromStr>(field: &[u8]) -> Option<T> {
    std::str::from_utf8(field).ok()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans(pairs: &[(usize, usize)]) -> Vec<Range<usize>> {
        pairs.iter().map(|&(start, end)| start..end).collect()
    }

    const LINES: &[u8] =
        b"src/a.rs\x001\x00foo bar foo\nsrc/a.rs\x003\x00let Foo = 1;\nb.txt\x002\x00food\n";
    const MATCHES: &[u8] =
        b"src/a.rs\x001\x001\x00foo\nsrc/a.rs\x001\x009\x00foo\nsrc/a.rs\x003\x005\x00Foo\nb.txt\x002\x001\x00foo\n";

    #[test]
    fn lines_and_their_matches_group_by_file_under_the_root() {
        let found = combine(Path::new("/r"), LINES, MATCHES, 10);
        assert!(!found.truncated);
        assert_eq!(found.match_count(), 4);
        assert_eq!(
            found.files,
            [
                FileMatches {
                    path: PathBuf::from("/r/src/a.rs"),
                    lines: vec![
                        FoundLine {
                            line: 1,
                            text: "foo bar foo".to_owned(),
                            matches: vec![0..3, 8..11],
                        },
                        FoundLine {
                            line: 3,
                            text: "let Foo = 1;".to_owned(),
                            matches: spans(&[(4, 7)]),
                        },
                    ],
                },
                FileMatches {
                    path: PathBuf::from("/r/b.txt"),
                    lines: vec![FoundLine {
                        line: 2,
                        text: "food".to_owned(),
                        matches: spans(&[(0, 3)]),
                    }],
                },
            ]
        );
    }

    #[test]
    fn the_line_limit_cuts_the_results_and_says_so() {
        let found = combine(Path::new("/r"), LINES, MATCHES, 2);
        assert!(found.truncated);
        assert_eq!(found.files.len(), 1);
        assert_eq!(found.files[0].lines.len(), 2);
    }

    #[test]
    fn spans_that_do_not_fit_the_line_are_dropped() {
        let matches = b"b.txt\x002\x003\x00foo\nb.txt\x002\x0040\x00foo\n";
        let found = combine(Path::new("/r"), b"b.txt\x002\x00food\n", matches, 10);
        assert_eq!(found, ProjectMatches::default());
    }
}
