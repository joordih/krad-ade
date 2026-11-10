use krad_domain::{GitObjectId, HunkFingerprint};
use std::path::PathBuf;

use super::{FilePatch, Hunk, HunkLine, HunkLineKind, PatchBody, PatchStatus};

const FILE_HEADER: &str = "diff --git ";
const OLD_PREFIX: &str = "a/";
const NEW_PREFIX: &str = " b/";
const QUOTE: char = '"';
const ESCAPE: char = '\\';
const INDEX_PREFIX: &str = "index ";
const INDEX_RANGE: &str = "..";
const NEW_FILE: &str = "new file mode ";
const RENAME_FROM: &str = "rename from ";
const RENAME_TO: &str = "rename to ";
const COPY_FROM: &str = "copy from ";
const COPY_TO: &str = "copy to ";
const OLD_FILE_LINE: &str = "--- a/";
const NEW_FILE_LINE: &str = "+++ b/";
const NAME_END: char = '\t';
const DELETED_FILE: &str = "deleted file mode ";
const BINARY_PATCH: &str = "GIT binary patch";
const BINARY_FILES: &str = "Binary files ";
const SUBMODULE_MODE: &str = "160000";
const HUNK_HEADER: &str = "@@ ";
const HUNK_END: &str = " @@";
const OLD_RANGE: char = '-';
const NEW_RANGE: char = '+';
const RANGE_COUNT: char = ',';
const CONTEXT_MARK: char = ' ';
const ADDED_MARK: char = '+';
const REMOVED_MARK: char = '-';
const NO_NEWLINE_MARK: char = '\\';
const NEWLINE: char = '\n';
const DEFAULT_RANGE_COUNT: &str = "1";
const C_ESCAPES: [(char, u8); 7] = [
    ('n', b'\n'),
    ('t', b'\t'),
    ('r', b'\r'),
    ('a', b'\x07'),
    ('b', b'\x08'),
    ('f', b'\x0c'),
    ('v', b'\x0b'),
];
const OCTAL_DIGITS: usize = 3;
const OCTAL_RADIX: u32 = 8;

#[must_use]
pub fn parse_patch(text: &str) -> Vec<FilePatch> {
    let mut sections: Vec<Vec<&str>> = Vec::new();
    for line in text.strip_suffix(NEWLINE).unwrap_or(text).split(NEWLINE) {
        match sections.last_mut() {
            Some(section) if !line.starts_with(FILE_HEADER) => section.push(line),
            _ if line.starts_with(FILE_HEADER) => sections.push(vec![line]),
            _ => {}
        }
    }
    sections
        .iter()
        .filter_map(|section| parse_file(section))
        .collect()
}

fn parse_file(section: &[&str]) -> Option<FilePatch> {
    let (first, rest) = section.split_first()?;
    let names = first.strip_prefix(FILE_HEADER)?;
    let header_end = rest
        .iter()
        .position(|line| line.starts_with(HUNK_HEADER))
        .unwrap_or(rest.len());
    let (header, body) = rest.split_at(header_end);
    let (path, renamed_from) = if let Some(path) = header_path(names) {
        (path, None)
    } else {
        let (from, to) = moved_paths(header)?;
        (to, Some(from))
    };
    let status = if header.iter().any(|line| line.starts_with(NEW_FILE)) {
        PatchStatus::Added
    } else if header.iter().any(|line| line.starts_with(DELETED_FILE)) {
        PatchStatus::Deleted
    } else {
        PatchStatus::Modified
    };
    let index = header
        .iter()
        .find_map(|line| line.strip_prefix(INDEX_PREFIX));
    let (old_id, new_id) = index.map_or((None, None), object_ids);
    let path_bytes = path.as_os_str().as_encoded_bytes();
    let fingerprint = HunkFingerprint::new(
        std::iter::once(path_bytes).chain(header.iter().map(|line| line.as_bytes())),
    );
    let submodule = header.iter().any(|line| line.ends_with(SUBMODULE_MODE));
    let binary = header
        .iter()
        .any(|line| line.starts_with(BINARY_PATCH) || line.starts_with(BINARY_FILES));
    let body = if let Some(from) = renamed_from {
        PatchBody::Renamed { from }
    } else if submodule {
        PatchBody::Submodule
    } else if binary {
        PatchBody::Binary
    } else {
        PatchBody::Text(parse_hunks(path_bytes, body))
    };
    Some(FilePatch {
        path,
        status,
        old_id,
        new_id,
        body,
        fingerprint,
    })
}

fn moved_paths(header: &[&str]) -> Option<(PathBuf, PathBuf)> {
    let named = |prefixes: [&str; 2], fallback: &str| {
        header
            .iter()
            .find_map(|line| prefixes.iter().find_map(|prefix| line.strip_prefix(prefix)))
            .and_then(header_name)
            .or_else(|| {
                header
                    .iter()
                    .find_map(|line| line.strip_prefix(fallback))
                    .map(|name| PathBuf::from(name.trim_end_matches(NAME_END)))
            })
    };
    let from = named([RENAME_FROM, COPY_FROM], OLD_FILE_LINE)?;
    let to = named([RENAME_TO, COPY_TO], NEW_FILE_LINE)?;
    Some((from, to))
}

fn header_name(name: &str) -> Option<PathBuf> {
    if name.starts_with(QUOTE) {
        return unquote(name).map(PathBuf::from);
    }
    Some(PathBuf::from(name))
}

fn object_ids(index: &str) -> (Option<GitObjectId>, Option<GitObjectId>) {
    let range = index.split_whitespace().next().unwrap_or_default();
    let Some((old, new)) = range.split_once(INDEX_RANGE) else {
        return (None, None);
    };
    let real = |raw: &str| {
        raw.parse::<GitObjectId>()
            .ok()
            .filter(|_| raw.bytes().any(|byte| byte != b'0'))
    };
    (real(old), real(new))
}

fn header_path(names: &str) -> Option<PathBuf> {
    if names.starts_with(QUOTE) {
        let quoted = unquote(names)?;
        return quoted.strip_prefix(OLD_PREFIX).map(PathBuf::from);
    }
    let inner = names.strip_prefix(OLD_PREFIX)?;
    let half = inner.len().checked_sub(NEW_PREFIX.len())? / 2;
    let path = inner.get(..half)?;
    let other = inner.get(half..)?.strip_prefix(NEW_PREFIX)?;
    (path == other).then(|| PathBuf::from(path))
}

fn unquote(quoted: &str) -> Option<String> {
    let mut chars = quoted.strip_prefix(QUOTE)?.chars();
    let mut bytes = Vec::new();
    while let Some(c) = chars.next() {
        match c {
            QUOTE => return Some(String::from_utf8_lossy(&bytes).into_owned()),
            ESCAPE => {
                let escaped = chars.next()?;
                let named = C_ESCAPES
                    .iter()
                    .find(|(name, _)| *name == escaped)
                    .map(|(_, byte)| *byte);
                let byte = match (named, escaped) {
                    (Some(byte), _) => byte,
                    (None, '0'..='7') => {
                        let digits: String = std::iter::once(escaped)
                            .chain(chars.by_ref().take(OCTAL_DIGITS - 1))
                            .collect();
                        u8::from_str_radix(&digits, OCTAL_RADIX).ok()?
                    }
                    (None, other) => u8::try_from(other).ok()?,
                };
                bytes.push(byte);
            }
            other => {
                let mut buffer = [0; 4];
                bytes.extend_from_slice(other.encode_utf8(&mut buffer).as_bytes());
            }
        }
    }
    None
}

fn range_start(range: &str) -> Option<(usize, usize)> {
    let (start, count) = range
        .split_once(RANGE_COUNT)
        .unwrap_or((range, DEFAULT_RANGE_COUNT));
    Some((start.parse().ok()?, count.parse().ok()?))
}

fn hunk_ranges(header: &str) -> Option<((usize, usize), (usize, usize))> {
    let ranges = header.strip_prefix(HUNK_HEADER)?;
    let ranges = &ranges[..ranges.find(HUNK_END)?];
    let (old, new) = ranges.split_once(CONTEXT_MARK)?;
    Some((
        range_start(old.strip_prefix(OLD_RANGE)?)?,
        range_start(new.strip_prefix(NEW_RANGE)?)?,
    ))
}

fn parse_hunks(path: &[u8], body: &[&str]) -> Vec<Hunk> {
    let mut hunks = Vec::new();
    let mut bases: Vec<HunkFingerprint> = Vec::new();
    let mut rest = body;
    while let Some((header, after)) = rest.split_first() {
        rest = after;
        let Some(((old_start, mut old_left), (new_start, mut new_left))) = hunk_ranges(header)
        else {
            continue;
        };
        let mut lines: Vec<HunkLine> = Vec::new();
        while let Some((line, after)) = rest.split_first() {
            if line.starts_with(NO_NEWLINE_MARK) {
                if let Some(last) = lines.last_mut() {
                    last.newline = false;
                }
                rest = after;
                continue;
            }
            if old_left == 0 && new_left == 0 {
                break;
            }
            let mut chars = line.chars();
            let kind = match chars.next() {
                Some(ADDED_MARK) => HunkLineKind::Added,
                Some(REMOVED_MARK) => HunkLineKind::Removed,
                Some(CONTEXT_MARK) | None => HunkLineKind::Context,
                Some(_) => break,
            };
            match kind {
                HunkLineKind::Context => {
                    old_left = old_left.saturating_sub(1);
                    new_left = new_left.saturating_sub(1);
                }
                HunkLineKind::Added => new_left = new_left.saturating_sub(1),
                HunkLineKind::Removed => old_left = old_left.saturating_sub(1),
            }
            lines.push(HunkLine {
                kind,
                text: chars.as_str().to_owned(),
                newline: true,
            });
            rest = after;
        }
        let base = hunk_fingerprint(path, &lines);
        let earlier = bases.iter().filter(|seen| **seen == base).count();
        bases.push(base);
        let fingerprint = base.occurrence(earlier);
        hunks.push(Hunk {
            old_start,
            new_start,
            lines,
            fingerprint,
        });
    }
    hunks
}

fn hunk_fingerprint(path: &[u8], lines: &[HunkLine]) -> HunkFingerprint {
    let marks: Vec<String> = lines
        .iter()
        .map(|line| {
            let mark = match line.kind {
                HunkLineKind::Context => CONTEXT_MARK,
                HunkLineKind::Added => ADDED_MARK,
                HunkLineKind::Removed => REMOVED_MARK,
            };
            let end = if line.newline {
                NEWLINE.to_string()
            } else {
                String::new()
            };
            format!("{mark}{}{end}", line.text)
        })
        .collect();
    HunkFingerprint::new(std::iter::once(path).chain(marks.iter().map(String::as_bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_file_section_becomes_a_patch_with_its_status_and_objects() {
        let text = concat!(
            "diff --git a/new file.txt b/new file.txt\n",
            "new file mode 100644\n",
            "index 0000000000000000000000000000000000000000..2222222222222222222222222222222222222222\n",
            "--- /dev/null\n",
            "+++ b/new file.txt\t\n",
            "@@ -0,0 +1 @@\n",
            "+hello\n",
            "diff --git a/gone.rs b/gone.rs\n",
            "deleted file mode 100644\n",
            "index 1111111111111111111111111111111111111111..0000000000000000000000000000000000000000\n",
            "--- a/gone.rs\n",
            "+++ /dev/null\n",
            "@@ -1,2 +0,0 @@\n",
            "-a\n",
            "-b\n",
            "diff --git a/logo.png b/logo.png\n",
            "index 1111111111111111111111111111111111111111..2222222222222222222222222222222222222222 100644\n",
            "GIT binary patch\n",
            "literal 3\n",
            "KcmZ?wW&i>J00II3\n",
            "\n",
            "literal 0\n",
            "HcmV?d00001\n",
            "\n",
            "diff --git \"a/tab\\there\" \"b/tab\\there\"\n",
            "index 1111111111111111111111111111111111111111..2222222222222222222222222222222222222222 160000\n",
            "--- \"a/tab\\there\"\n",
            "+++ \"b/tab\\there\"\n",
            "@@ -1 +1 @@\n",
            "-Subproject commit 1111111111111111111111111111111111111111\n",
            "+Subproject commit 2222222222222222222222222222222222222222\n",
        );
        let files = parse_patch(text);
        let summary: Vec<(PathBuf, PatchStatus, bool, bool)> = files
            .iter()
            .map(|file| {
                (
                    file.path.clone(),
                    file.status,
                    file.old_id.is_some(),
                    file.new_id.is_some(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                (
                    PathBuf::from("new file.txt"),
                    PatchStatus::Added,
                    false,
                    true
                ),
                (PathBuf::from("gone.rs"), PatchStatus::Deleted, true, false),
                (PathBuf::from("logo.png"), PatchStatus::Modified, true, true),
                (
                    PathBuf::from("tab\there"),
                    PatchStatus::Modified,
                    true,
                    true
                ),
            ]
        );
        assert_eq!(files[0].hunks().len(), 1);
        assert_eq!(files[0].hunks()[0].new_start, 1);
        assert_eq!(files[1].hunks()[0].lines.len(), 2);
        assert_eq!(files[2].body, PatchBody::Binary);
        assert_eq!(files[3].body, PatchBody::Submodule);
    }

    #[test]
    fn a_removed_line_that_looks_like_a_header_stays_in_its_hunk() {
        let text = concat!(
            "diff --git a/a.md b/a.md\n",
            "index 1111111111111111111111111111111111111111..2222222222222222222222222222222222222222 100644\n",
            "--- a/a.md\n",
            "+++ b/a.md\n",
            "@@ -1,2 +1,1 @@\n",
            "--- a/b\n",
            " keep\n",
        );
        let file = parse_patch(text).remove(0);
        let hunk = &file.hunks()[0];
        assert_eq!(hunk.lines[0].kind, HunkLineKind::Removed);
        assert_eq!(hunk.lines[0].text, "-- a/b");
        assert_eq!(hunk.lines.len(), 2);
    }

    #[test]
    fn a_renamed_and_edited_file_is_kept_as_a_rename_without_hunks() {
        let text = concat!(
            "diff --git a/src/old.rs b/src/new.rs\n",
            "similarity index 90%\n",
            "rename from src/old.rs\n",
            "rename to src/new.rs\n",
            "index 1111111111111111111111111111111111111111..2222222222222222222222222222222222222222 100644\n",
            "--- a/src/old.rs\n",
            "+++ b/src/new.rs\n",
            "@@ -1 +1 @@\n",
            "-old\n",
            "+new\n",
            "diff --git a/keep.rs b/keep.rs\n",
            "index 1111111111111111111111111111111111111111..2222222222222222222222222222222222222222 100644\n",
            "--- a/keep.rs\n",
            "+++ b/keep.rs\n",
            "@@ -1 +1 @@\n",
            "-a\n",
            "+b\n",
        );
        let files = parse_patch(text);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, PathBuf::from("src/new.rs"));
        assert_eq!(
            files[0].body,
            PatchBody::Renamed {
                from: PathBuf::from("src/old.rs")
            }
        );
        assert_eq!(files[0].hunks(), []);
        assert_eq!(files[0].fingerprints().len(), 1);
        assert_eq!(files[1].hunks().len(), 1);
    }

    #[test]
    fn a_rename_without_rename_lines_reads_its_names_from_the_file_lines() {
        let text = concat!(
            "diff --git a/a b c b/d e\n",
            "--- a/a b c\t\n",
            "+++ b/d e\t\n",
            "@@ -1 +1 @@\n",
            "-x\n",
            "+y\n",
        );
        let files = parse_patch(text);
        assert_eq!(files[0].path, PathBuf::from("d e"));
        assert_eq!(
            files[0].body,
            PatchBody::Renamed {
                from: PathBuf::from("a b c")
            }
        );
    }

    #[test]
    fn identical_hunks_of_one_file_get_their_own_prints() {
        let text = concat!(
            "diff --git a/a.rs b/a.rs\n",
            "index 1111111111111111111111111111111111111111..2222222222222222222222222222222222222222 100644\n",
            "--- a/a.rs\n",
            "+++ b/a.rs\n",
            "@@ -1 +1 @@\n",
            "-x\n",
            "+y\n",
            "@@ -9 +9 @@\n",
            "-x\n",
            "+y\n",
        );
        let file = parse_patch(text).remove(0);
        let prints = file.fingerprints();
        assert_eq!(prints.len(), 2);
        assert_ne!(prints[0], prints[1]);
        let lone = &text[..text.find("@@ -9").unwrap_or(text.len())];
        assert_eq!(parse_patch(lone).remove(0).fingerprints(), [prints[0]]);
    }

    #[test]
    fn quoted_names_unescape_octal_bytes() {
        assert_eq!(
            unquote("\"a/caf\\303\\251\" \"b/caf\\303\\251\"").as_deref(),
            Some("a/café")
        );
    }
}
