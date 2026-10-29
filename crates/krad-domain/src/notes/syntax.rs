const FENCE_MARKS: [&str; 2] = ["```", "~~~"];
const INLINE_CODE: char = '`';
const WIKI_OPEN: &str = "[[";
const WIKI_CLOSE: &str = "]]";
const EMBED_MARK: char = '!';
const ALIAS_MARK: char = '|';
const HEADING_MARK: char = '#';
const TAG_MARK: char = '#';
const TAG_NESTING: char = '/';
const MAX_HEADING: usize = 6;
const URL_SCHEME_END: &str = "://";
const MAIL_SCHEME: &str = "mailto:";
const ENCODED_SPACE: &str = "%20";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkSyntax {
    Wiki,
    Markdown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteLink {
    pub target: String,
    pub heading: Option<String>,
    pub alias: Option<String>,
    pub embed: bool,
    pub syntax: LinkSyntax,
    pub line: usize,
}

impl NoteLink {
    #[must_use]
    pub fn is_external(&self) -> bool {
        self.target.contains(URL_SCHEME_END) || self.target.starts_with(MAIL_SCHEME)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heading {
    pub level: u8,
    pub text: String,
    pub line: usize,
}

fn prose_lines(body: &str) -> impl Iterator<Item = (usize, String)> + '_ {
    let mut fence: Option<&str> = None;
    body.lines().enumerate().filter_map(move |(number, line)| {
        let trimmed = line.trim_start();
        if let Some(mark) = FENCE_MARKS.iter().find(|mark| trimmed.starts_with(**mark)) {
            fence = match fence {
                Some(open) if open == *mark => None,
                Some(open) => Some(open),
                None => Some(mark),
            };
            return None;
        }
        if fence.is_some() {
            return None;
        }
        Some((number, without_inline_code(line)))
    })
}

fn without_inline_code(line: &str) -> String {
    let mut inside = false;
    line.chars()
        .map(|character| {
            if character == INLINE_CODE {
                inside = !inside;
                ' '
            } else if inside {
                ' '
            } else {
                character
            }
        })
        .collect()
}

fn decode(target: &str) -> String {
    target.trim().replace(ENCODED_SPACE, " ")
}

fn split_heading(target: &str) -> (String, Option<String>) {
    match target.split_once(HEADING_MARK) {
        Some((note, heading)) => (
            decode(note),
            Some(heading.trim().to_owned()).filter(|heading| !heading.is_empty()),
        ),
        None => (decode(target), None),
    }
}

fn wiki_links(line: &str, number: usize, links: &mut Vec<NoteLink>) {
    let mut rest = line;
    let mut offset = 0;
    while let Some(open) = rest.find(WIKI_OPEN) {
        let after = &rest[open + WIKI_OPEN.len()..];
        let Some(close) = after.find(WIKI_CLOSE) else {
            break;
        };
        let inner = &after[..close];
        let embed = line[..offset + open].ends_with(EMBED_MARK);
        let (target, alias) = match inner.split_once(ALIAS_MARK) {
            Some((target, alias)) => (target, Some(alias.trim().to_owned())),
            None => (inner, None),
        };
        let (target, heading) = split_heading(target);
        if !target.is_empty() || heading.is_some() {
            links.push(NoteLink {
                target,
                heading,
                alias,
                embed,
                syntax: LinkSyntax::Wiki,
                line: number,
            });
        }
        let consumed = open + WIKI_OPEN.len() + close + WIKI_CLOSE.len();
        offset += consumed;
        rest = &rest[consumed..];
    }
}

fn markdown_links(line: &str, number: usize, links: &mut Vec<NoteLink>) {
    let bytes: Vec<char> = line.chars().collect();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != '[' || bytes.get(index + 1) == Some(&'[') {
            index += 1;
            continue;
        }
        let Some(text_end) = (index + 1..bytes.len()).find(|at| bytes[*at] == ']') else {
            break;
        };
        if bytes.get(text_end + 1) != Some(&'(') {
            index = text_end + 1;
            continue;
        }
        let Some(target_end) = (text_end + 2..bytes.len()).find(|at| bytes[*at] == ')') else {
            break;
        };
        let target: String = bytes[text_end + 2..target_end].iter().collect();
        let alias: String = bytes[index + 1..text_end].iter().collect();
        let (target, heading) = split_heading(&target);
        if !target.is_empty() || heading.is_some() {
            links.push(NoteLink {
                target,
                heading,
                alias: Some(alias).filter(|alias| !alias.is_empty()),
                embed: index > 0 && bytes[index - 1] == EMBED_MARK,
                syntax: LinkSyntax::Markdown,
                line: number,
            });
        }
        index = target_end + 1;
    }
}

#[must_use]
pub fn note_links(body: &str) -> Vec<NoteLink> {
    let mut links = Vec::new();
    for (number, line) in prose_lines(body) {
        let mut on_line = Vec::new();
        wiki_links(&line, number, &mut on_line);
        markdown_links(&line, number, &mut on_line);
        links.extend(on_line);
    }
    links
}

fn heading_of(line: &str, number: usize) -> Option<Heading> {
    let marks = line.chars().take_while(|c| *c == HEADING_MARK).count();
    if marks == 0 || marks > MAX_HEADING {
        return None;
    }
    let text = line[marks..]
        .strip_prefix(' ')?
        .trim()
        .trim_end_matches(HEADING_MARK)
        .trim();
    Some(Heading {
        level: u8::try_from(marks).ok()?,
        text: text.to_owned(),
        line: number,
    })
}

#[must_use]
pub fn outline(body: &str) -> Vec<Heading> {
    prose_lines(body)
        .filter_map(|(number, line)| heading_of(&line, number))
        .collect()
}

fn is_tag_char(character: char) -> bool {
    character.is_alphanumeric() || matches!(character, '_' | '-' | TAG_NESTING)
}

#[must_use]
pub fn note_tags(body: &str) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for (number, line) in prose_lines(body) {
        let start = if heading_of(&line, number).is_some() {
            line.find(' ').unwrap_or(line.len())
        } else {
            0
        };
        let chars: Vec<char> = line[start..].chars().collect();
        for (index, character) in chars.iter().enumerate() {
            let starts = *character == TAG_MARK && (index == 0 || chars[index - 1].is_whitespace());
            if !starts {
                continue;
            }
            let tag: String = chars[index + 1..]
                .iter()
                .take_while(|c| is_tag_char(**c))
                .collect();
            let tag = tag.trim_end_matches(TAG_NESTING).to_owned();
            if tag.chars().any(|c| !c.is_ascii_digit()) && !tags.contains(&tag) {
                tags.push(tag);
            }
        }
    }
    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    fn targets(body: &str) -> Vec<(String, Option<String>, bool, LinkSyntax)> {
        note_links(body)
            .into_iter()
            .map(|link| (link.target, link.heading, link.embed, link.syntax))
            .collect()
    }

    #[test]
    fn every_link_form_is_read() {
        let body = "See [[Architecture]] and [[Plan#Phase 2|the plan]].\n\
                    ![[diagram.png]] then [guide](how-to%20guides.md#setup) and ![shot](attachments/a.png).\n\
                    [[#Local heading]] and [site](https://example.com).";
        assert_eq!(
            targets(body),
            [
                ("Architecture".to_owned(), None, false, LinkSyntax::Wiki),
                (
                    "Plan".to_owned(),
                    Some("Phase 2".to_owned()),
                    false,
                    LinkSyntax::Wiki
                ),
                ("diagram.png".to_owned(), None, true, LinkSyntax::Wiki),
                (
                    "how-to guides.md".to_owned(),
                    Some("setup".to_owned()),
                    false,
                    LinkSyntax::Markdown
                ),
                (
                    "attachments/a.png".to_owned(),
                    None,
                    true,
                    LinkSyntax::Markdown
                ),
                (
                    String::new(),
                    Some("Local heading".to_owned()),
                    false,
                    LinkSyntax::Wiki
                ),
                (
                    "https://example.com".to_owned(),
                    None,
                    false,
                    LinkSyntax::Markdown
                ),
            ]
        );
        let links = note_links(body);
        assert_eq!(links[1].alias.as_deref(), Some("the plan"));
        assert!(links[6].is_external() && !links[0].is_external());
        assert_eq!(links[2].line, 1);
    }

    #[test]
    fn code_holds_no_links_tags_or_headings() {
        let body = "```\n[[Inside]] #tag\n# not a heading\n```\n`[[inline]] #code` [[Outside]]\n";
        assert_eq!(targets(body).len(), 1);
        assert_eq!(note_tags(body), Vec::<String>::new());
        assert_eq!(outline(body), Vec::<Heading>::new());
    }

    #[test]
    fn tags_skip_headings_numbers_and_repeats() {
        let body = "# Title #heading-tag\n#project/krad and #rust, #2026 #rust\nissue#3 is no tag";
        assert_eq!(note_tags(body), ["heading-tag", "project/krad", "rust"]);
    }

    #[test]
    fn the_outline_has_each_heading_with_its_level_and_line() {
        let body = "# One\ntext\n## Two ##\n####### seven\n#nospace";
        assert_eq!(
            outline(body),
            [
                Heading {
                    level: 1,
                    text: "One".to_owned(),
                    line: 0
                },
                Heading {
                    level: 2,
                    text: "Two".to_owned(),
                    line: 2
                },
            ]
        );
    }
}
