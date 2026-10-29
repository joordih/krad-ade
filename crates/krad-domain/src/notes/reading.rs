use std::ops::Range;

mod footnotes;

use super::{note_links, LinkSyntax, NoteLink, VaultIndex, ATTACHMENTS_DIR};
use footnotes::link_footnotes;

pub const NOTE_SCHEME: &str = "krad-note:";
pub const FOOTNOTE_SCHEME: &str = "krad-footnote:";
pub const FOOTNOTE_BACK_SCHEME: &str = "krad-footnote-back:";
pub const OPEN_SCHEME: &str = "krad-open:";
pub const EMBED_CALLOUT: &str = "embed";
pub const VIDEO_CALLOUT: &str = "video";

pub const IMAGE_EXTENSIONS: [&str; 8] = ["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "avif"];
pub const VIDEO_EXTENSIONS: [&str; 6] = ["mp4", "mov", "webm", "mkv", "m4v", "avi"];

const MAX_EMBED_DEPTH: usize = 2;
const FENCE_MARKS: [&str; 2] = ["```", "~~~"];
const QUOTE_MARK: &str = "> ";
const PATH_SEPARATOR: char = '/';
const PARENT_STEP: &str = "../";
const SPACE_ESCAPE: &str = "%20";
const HEADING_MARK: char = '#';
const EXTENSION_DOT: char = '.';
const WIKI_OPEN: &str = "[[";
const WIKI_CLOSE: &str = "]]";
const EMBED_MARK: char = '!';

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReadingDoc {
    pub markdown: String,
    pub footnote_definitions: Vec<(String, Range<usize>)>,
    pub footnote_marks: Vec<(String, Range<usize>)>,
    pub headings: Vec<Range<usize>>,
}

impl ReadingDoc {
    #[must_use]
    pub fn footnote_definition(&self, id: &str) -> Option<Range<usize>> {
        self.footnote_definitions
            .iter()
            .find(|(known, _)| known == id)
            .map(|(_, range)| range.clone())
    }

    #[must_use]
    pub fn footnote_mark(&self, id: &str) -> Option<Range<usize>> {
        self.footnote_marks
            .iter()
            .find(|(known, _)| known == id)
            .map(|(_, range)| range.clone())
    }
}

fn extension(path: &str) -> Option<String> {
    let name = path.rsplit(PATH_SEPARATOR).next()?;
    name.rsplit_once(EXTENSION_DOT)
        .map(|(_, extension)| extension.to_ascii_lowercase())
}

fn escape(path: &str) -> String {
    path.replace(' ', SPACE_ESCAPE)
}

fn relative_to(from: &str, path: &str) -> String {
    let depth = from.matches(PATH_SEPARATOR).count();
    format!("{}{path}", PARENT_STEP.repeat(depth))
}

fn attachment_path(target: &str) -> String {
    if target.contains(PATH_SEPARATOR) {
        target.to_owned()
    } else {
        format!("{ATTACHMENTS_DIR}{PATH_SEPARATOR}{target}")
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit(PATH_SEPARATOR).next().unwrap_or(path)
}

fn section<'a>(body: &'a str, heading: &str) -> Option<&'a str> {
    let mut start = None;
    let mut level = 0;
    let mut offset = 0;
    for line in body.split_inclusive('\n') {
        let marks = line.chars().take_while(|c| *c == HEADING_MARK).count();
        let is_heading = marks > 0 && line[marks..].starts_with(' ');
        if is_heading {
            if let Some(from) = start {
                if marks <= level {
                    return body.get(from..offset);
                }
            } else if line[marks..].trim().eq_ignore_ascii_case(heading.trim()) {
                start = Some(offset);
                level = marks;
            }
        }
        offset += line.len();
    }
    start.and_then(|from| body.get(from..))
}

struct Rewriter<'a> {
    index: &'a VaultIndex,
    from: &'a str,
    depth: usize,
}

impl Rewriter<'_> {
    fn link(&self, link: &NoteLink, source: &str) -> String {
        if link.embed {
            return self.embed(link);
        }
        let label =
            link.alias
                .clone()
                .unwrap_or_else(|| match (&link.heading, link.target.is_empty()) {
                    (Some(heading), true) => heading.clone(),
                    (Some(heading), false) => format!("{} › {heading}", link.target),
                    (None, _) => link.target.clone(),
                });
        let target = self
            .index
            .resolve(self.from, link)
            .map_or_else(|| link.target.clone(), str::to_owned);
        let heading = link
            .heading
            .as_ref()
            .map_or_else(String::new, |heading| format!("{HEADING_MARK}{heading}"));
        if link.syntax == LinkSyntax::Markdown && link.is_external() {
            return source.to_owned();
        }
        format!(
            "[{label}]({NOTE_SCHEME}{}{})",
            escape(&target),
            escape(&heading)
        )
    }

    fn embed(&self, link: &NoteLink) -> String {
        let extension = extension(&link.target);
        let is_image = extension
            .as_deref()
            .is_some_and(|ext| IMAGE_EXTENSIONS.contains(&ext));
        let is_video = extension
            .as_deref()
            .is_some_and(|ext| VIDEO_EXTENSIONS.contains(&ext));
        if link.is_external() {
            let alt = link.alias.clone().unwrap_or_default();
            return format!("![{alt}]({})", link.target);
        }
        if is_image || is_video {
            let path = match link.syntax {
                LinkSyntax::Wiki => attachment_path(&link.target),
                LinkSyntax::Markdown => {
                    let folder = self.index.note(self.from).map_or("", |note| note.folder());
                    if folder.is_empty() {
                        link.target.clone()
                    } else {
                        format!("{folder}{PATH_SEPARATOR}{}", link.target)
                    }
                }
            };
            let shown = relative_to(self.from, &path);
            if is_video {
                return format!(
                    "\n> [!{VIDEO_CALLOUT}] [{}]({OPEN_SCHEME}{})\n",
                    file_name(&path),
                    escape(&path)
                );
            }
            let alt = link.alias.clone().unwrap_or_default();
            return format!("![{alt}]({})", escape(&shown));
        }
        let embedded = self
            .index
            .resolve(self.from, link)
            .and_then(|path| self.index.note(path));
        let Some(note) = embedded.filter(|_| self.depth < MAX_EMBED_DEPTH) else {
            let mut plain = link.clone();
            plain.embed = false;
            return self.link(&plain, "");
        };
        let body = link
            .heading
            .as_deref()
            .and_then(|heading| section(&note.body, heading))
            .unwrap_or(&note.body);
        let inner = Rewriter {
            index: self.index,
            from: &note.path,
            depth: self.depth + 1,
        }
        .rewrite_body(body);
        let quoted: Vec<String> = inner
            .lines()
            .map(|line| format!("{QUOTE_MARK}{line}").trim_end().to_owned())
            .collect();
        format!(
            "\n> [!{EMBED_CALLOUT}] [{}]({NOTE_SCHEME}{})\n{}\n",
            note.title,
            escape(&note.path),
            quoted.join("\n")
        )
    }

    fn rewrite_body(&self, body: &str) -> String {
        let links = note_links(body);
        let mut out = String::with_capacity(body.len());
        let mut fence: Option<&str> = None;
        for (number, line) in body.split_inclusive('\n').enumerate() {
            let trimmed = line.trim_start();
            if let Some(mark) = FENCE_MARKS.iter().find(|mark| trimmed.starts_with(**mark)) {
                fence = match fence {
                    Some(open) if open == *mark => None,
                    Some(open) => Some(open),
                    None => Some(mark),
                };
                out.push_str(line);
                continue;
            }
            if fence.is_some() {
                out.push_str(line);
                continue;
            }
            let on_line: Vec<&NoteLink> = links.iter().filter(|link| link.line == number).collect();
            out.push_str(&self.rewrite_line(line, &on_line));
        }
        out
    }

    fn rewrite_line(&self, line: &str, links: &[&NoteLink]) -> String {
        let mut out = String::new();
        let mut rest = line;
        for link in links {
            let Some((start, end)) = find_link(rest, link) else {
                continue;
            };
            out.push_str(&rest[..start]);
            out.push_str(&self.link(link, &rest[start..end]));
            rest = &rest[end..];
        }
        out.push_str(rest);
        out
    }
}

fn find_link(text: &str, link: &NoteLink) -> Option<(usize, usize)> {
    match link.syntax {
        LinkSyntax::Wiki => {
            let open = text.find(WIKI_OPEN)?;
            let close = text[open..].find(WIKI_CLOSE)? + open + WIKI_CLOSE.len();
            let start = if open > 0 && text[..open].ends_with(EMBED_MARK) {
                open - 1
            } else {
                open
            };
            Some((start, close))
        }
        LinkSyntax::Markdown => {
            let mut search = 0;
            while let Some(found) = text[search..].find('[') {
                let open = search + found;
                if text[open..].starts_with(WIKI_OPEN) {
                    search = open + WIKI_OPEN.len();
                    continue;
                }
                let text_end = text[open..].find("](")? + open;
                let close = text[text_end..].find(')')? + text_end + 1;
                let start = if open > 0 && text[..open].ends_with(EMBED_MARK) {
                    open - 1
                } else {
                    open
                };
                return Some((start, close));
            }
            None
        }
    }
}

fn heading_ranges(markdown: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut offset = 0;
    let mut fence = false;
    for line in markdown.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if FENCE_MARKS.iter().any(|mark| trimmed.starts_with(mark)) {
            fence = !fence;
        }
        let marks = line.chars().take_while(|c| *c == HEADING_MARK).count();
        if !fence && marks > 0 && line[marks..].starts_with(' ') {
            ranges.push(offset..offset + line.trim_end().len());
        }
        offset += line.len();
    }
    ranges
}

#[must_use]
pub fn reading_markdown(index: &VaultIndex, from: &str, body: &str) -> ReadingDoc {
    let rewritten = Rewriter {
        index,
        from,
        depth: 0,
    }
    .rewrite_body(body);
    let linked = link_footnotes(&rewritten);
    let headings = heading_ranges(&linked.markdown);
    ReadingDoc {
        markdown: linked.markdown,
        footnote_definitions: linked.definitions,
        footnote_marks: linked.marks,
        headings,
    }
}

#[must_use]
pub fn note_link_target(url: &str) -> Option<(String, Option<String>)> {
    let target = url.strip_prefix(NOTE_SCHEME)?.replace(SPACE_ESCAPE, " ");
    Some(match target.split_once(HEADING_MARK) {
        Some((path, heading)) => (path.to_owned(), Some(heading.to_owned())),
        None => (target, None),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::Note;

    fn vault() -> VaultIndex {
        VaultIndex::new([
            (
                "Plan.md".to_owned(),
                Note::parse("# Plan\n\nIntro.\n\n## Phase 2\n\nDo it.\n\n## Phase 3\n\nLater.\n"),
            ),
            (
                "Ideas/Spark.md".to_owned(),
                Note::parse("# Spark\n\n![[Plan#Phase 2]]\n"),
            ),
            ("Loop.md".to_owned(), Note::parse("![[Loop]]\n")),
        ])
    }

    #[test]
    fn wikilinks_become_links_the_app_follows() {
        let doc = reading_markdown(
            &vault(),
            "Ideas/Spark.md",
            "See [[Plan]], [[Plan#Phase 3|later]] and [[Missing]].\n",
        );
        assert_eq!(
            doc.markdown,
            "See [Plan](krad-note:Plan.md), [later](krad-note:Plan.md#Phase%203) and [Missing](krad-note:Missing).\n"
        );
        assert_eq!(
            note_link_target("krad-note:Plan.md#Phase%203"),
            Some(("Plan.md".to_owned(), Some("Phase 3".to_owned())))
        );
    }

    #[test]
    fn embeds_show_notes_sections_images_and_videos() {
        let index = vault();
        let doc = reading_markdown(
            &index,
            "Ideas/Spark.md",
            "![[Plan#Phase 2]]\n![[shot one.png]] ![[demo.mp4]]\n",
        );
        assert!(
            doc.markdown
                .contains("> [!embed] [Plan](krad-note:Plan.md)\n> ## Phase 2\n>\n> Do it."),
            "{}",
            doc.markdown
        );
        assert!(
            !doc.markdown.contains("Later."),
            "only the section is embedded"
        );
        assert!(
            doc.markdown.contains("![](../attachments/shot%20one.png)"),
            "{}",
            doc.markdown
        );
        assert!(doc
            .markdown
            .contains("> [!video] [demo.mp4](krad-open:attachments/demo.mp4)"));
    }

    #[test]
    fn a_note_that_embeds_itself_stops_at_the_depth_limit() {
        let doc = reading_markdown(&vault(), "Loop.md", "![[Loop]]\n");
        assert_eq!(doc.markdown.matches("[!embed]").count(), MAX_EMBED_DEPTH);
    }

    #[test]
    fn footnotes_link_both_ways_and_code_is_left_alone() {
        let body = "Claim[^1] and again[^1].\n\n```\n[[Plan]] [^1]\n```\n\n[^1]: The source.\n";
        let doc = reading_markdown(&vault(), "Plan.md", body);
        assert!(doc.markdown.contains("Claim[\\[1\\]](krad-footnote:1)"));
        assert!(doc.markdown.contains("```\n[[Plan]] [^1]\n```"));
        let definition = doc.footnote_definition("1").unwrap_or_default();
        assert_eq!(
            &doc.markdown[definition],
            "**1.** The source. [↩](krad-footnote-back:1)\n"
        );
        let mark = doc.footnote_mark("1").unwrap_or_default();
        assert_eq!(&doc.markdown[mark], "[\\[1\\]](krad-footnote:1)");
    }

    #[test]
    fn headings_are_found_in_order() {
        let doc = reading_markdown(&vault(), "Plan.md", "# One\ntext\n```\n# no\n```\n## Two\n");
        let headings: Vec<&str> = doc
            .headings
            .iter()
            .map(|range| &doc.markdown[range.clone()])
            .collect();
        assert_eq!(headings, ["# One", "## Two"]);
    }
}
