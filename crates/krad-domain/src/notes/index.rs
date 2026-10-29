use super::{
    note_links, note_tags, title_from_stem, LinkSyntax, Note, NoteLink, KEY_ALIASES, KEY_TAGS,
    NOTE_EXTENSION,
};

const PATH_SEPARATOR: char = '/';
const CURRENT_DIR: &str = ".";
const PARENT_DIR: &str = "..";
const EXTENSION_DOT: char = '.';

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultNote {
    pub path: String,
    pub title: String,
    pub aliases: Vec<String>,
    pub tags: Vec<String>,
    pub links: Vec<(NoteLink, String)>,
    pub body: String,
    pub generated: bool,
    pub edited: bool,
}

impl VaultNote {
    #[must_use]
    pub fn stem(&self) -> &str {
        let name = self
            .path
            .rsplit(PATH_SEPARATOR)
            .next()
            .unwrap_or(&self.path);
        name.strip_suffix(&format!("{EXTENSION_DOT}{NOTE_EXTENSION}"))
            .unwrap_or(name)
    }

    #[must_use]
    pub fn folder(&self) -> &str {
        self.path
            .rsplit_once(PATH_SEPARATOR)
            .map_or("", |(folder, _)| folder)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Backlink {
    pub from: String,
    pub line: usize,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VaultIndex {
    notes: Vec<VaultNote>,
}

fn normalise(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split(PATH_SEPARATOR) {
        match part {
            "" | CURRENT_DIR => {}
            PARENT_DIR => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    Some(parts.join(&PATH_SEPARATOR.to_string()))
}

fn has_extension(path: &str) -> bool {
    path.rsplit(PATH_SEPARATOR)
        .next()
        .is_some_and(|name| name.contains(EXTENSION_DOT))
}

impl VaultIndex {
    #[must_use]
    pub fn new(notes: impl IntoIterator<Item = (String, Note)>) -> Self {
        let mut notes: Vec<VaultNote> = notes
            .into_iter()
            .map(|(path, note)| {
                let body_lines: Vec<&str> = note.body.lines().collect();
                let links = note_links(&note.body)
                    .into_iter()
                    .map(|link| {
                        let text = body_lines
                            .get(link.line)
                            .map_or_default(|line| line.trim().to_owned());
                        (link, text)
                    })
                    .collect();
                let mut tags = note.properties.list(KEY_TAGS);
                for tag in note_tags(&note.body) {
                    if !tags.contains(&tag) {
                        tags.push(tag);
                    }
                }
                let stem = path
                    .rsplit(PATH_SEPARATOR)
                    .next()
                    .unwrap_or(&path)
                    .trim_end_matches(&format!("{EXTENSION_DOT}{NOTE_EXTENSION}"))
                    .to_owned();
                VaultNote {
                    generated: note.is_generated(),
                    edited: note.is_edited(),
                    body: note.body.clone(),
                    title: note.title(&title_from_stem(&stem)),
                    aliases: note.properties.list(KEY_ALIASES),
                    tags,
                    links,
                    path,
                }
            })
            .collect();
        notes.sort_by_key(|note| note.path.to_lowercase());
        Self { notes }
    }

    #[must_use]
    pub fn notes(&self) -> &[VaultNote] {
        &self.notes
    }

    #[must_use]
    pub fn note(&self, path: &str) -> Option<&VaultNote> {
        self.notes.iter().find(|note| note.path == path)
    }

    #[must_use]
    pub fn resolve(&self, from: &str, link: &NoteLink) -> Option<&str> {
        if link.is_external() {
            return None;
        }
        if link.target.is_empty() {
            return self.note(from).map(|note| note.path.as_str());
        }
        let from_folder = self.note(from).map_or("", VaultNote::folder);
        match link.syntax {
            LinkSyntax::Markdown => {
                let joined = if from_folder.is_empty() {
                    link.target.clone()
                } else {
                    format!("{from_folder}{PATH_SEPARATOR}{}", link.target)
                };
                let path = normalise(&joined)?;
                let path = if has_extension(&path) {
                    path
                } else {
                    format!("{path}{EXTENSION_DOT}{NOTE_EXTENSION}")
                };
                self.note(&path).map(|note| note.path.as_str())
            }
            LinkSyntax::Wiki => self.resolve_name(from_folder, &link.target),
        }
    }

    fn resolve_name(&self, from_folder: &str, name: &str) -> Option<&str> {
        let name = name
            .trim()
            .trim_end_matches(&format!("{EXTENSION_DOT}{NOTE_EXTENSION}"));
        if name.contains(PATH_SEPARATOR) {
            let wanted = format!("{}{EXTENSION_DOT}{NOTE_EXTENSION}", normalise(name)?);
            return self
                .notes
                .iter()
                .find(|note| {
                    note.path.eq_ignore_ascii_case(&wanted)
                        || note
                            .path
                            .to_lowercase()
                            .ends_with(&format!("{PATH_SEPARATOR}{}", wanted.to_lowercase()))
                })
                .map(|note| note.path.as_str());
        }
        let by_stem: Vec<&VaultNote> = self
            .notes
            .iter()
            .filter(|note| note.stem().eq_ignore_ascii_case(name))
            .collect();
        let candidates = if by_stem.is_empty() {
            self.notes
                .iter()
                .filter(|note| {
                    note.aliases
                        .iter()
                        .any(|alias| alias.eq_ignore_ascii_case(name))
                })
                .collect()
        } else {
            by_stem
        };
        candidates
            .into_iter()
            .min_by_key(|note| {
                (
                    note.folder() != from_folder,
                    note.path.matches(PATH_SEPARATOR).count(),
                    note.path.clone(),
                )
            })
            .map(|note| note.path.as_str())
    }

    #[must_use]
    pub fn backlinks(&self, path: &str) -> Vec<Backlink> {
        self.notes
            .iter()
            .filter(|note| note.path != path)
            .flat_map(|note| {
                note.links
                    .iter()
                    .filter(|(link, _)| self.resolve(&note.path, link) == Some(path))
                    .map(|(link, text)| Backlink {
                        from: note.path.clone(),
                        line: link.line,
                        text: text.clone(),
                    })
            })
            .collect()
    }

    #[must_use]
    pub fn tags(&self) -> Vec<(String, usize)> {
        let mut counts: Vec<(String, usize)> = Vec::new();
        for tag in self.notes.iter().flat_map(|note| &note.tags) {
            match counts
                .iter_mut()
                .find(|(known, _)| known.eq_ignore_ascii_case(tag))
            {
                Some((_, count)) => *count += 1,
                None => counts.push((tag.clone(), 1)),
            }
        }
        counts.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
        });
        counts
    }

    #[must_use]
    pub fn has_tag(&self, path: &str, tag: &str) -> bool {
        self.note(path).is_some_and(|note| {
            note.tags.iter().any(|own| {
                own.eq_ignore_ascii_case(tag)
                    || own
                        .to_lowercase()
                        .starts_with(&format!("{}{PATH_SEPARATOR}", tag.to_lowercase()))
            })
        })
    }

    #[must_use]
    pub fn find(&self, query: &str, limit: usize) -> Vec<&VaultNote> {
        let query = query.trim().to_lowercase();
        let mut found: Vec<(bool, usize, &VaultNote)> = self
            .notes
            .iter()
            .filter_map(|note| {
                let names = std::iter::once(note.title.as_str())
                    .chain(std::iter::once(note.stem()))
                    .chain(note.aliases.iter().map(String::as_str))
                    .map(str::to_lowercase);
                let mut best: Option<(bool, usize)> = None;
                for name in names {
                    if name.contains(&query) {
                        let rank = (!name.starts_with(&query), name.len());
                        best = Some(best.map_or(rank, |known: (bool, usize)| known.min(rank)));
                    }
                }
                best.map(|(starts, length)| (starts, length, note))
            })
            .collect();
        found.sort_by_key(|(starts, length, note)| (*starts, *length, note.path.clone()));
        found
            .into_iter()
            .take(limit)
            .map(|(_, _, note)| note)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault() -> VaultIndex {
        let note = |text: &str| Note::parse(text);
        VaultIndex::new([
            ("Plan.md".to_owned(), note("# Plan\n\nSee [[Architecture]] and [[Ideas]].\n#project\n")),
            ("Docs/Architecture.md".to_owned(), note("---\naliases: [Arch]\ntags: [design]\n---\n# Architecture\nBack to [the plan](../Plan.md).\n")),
            ("Ideas.md".to_owned(), note("[[Arch]] and [[Docs/Architecture#Crates]] #project/krad\n")),
            ("Archive/Ideas.md".to_owned(), note("Old ideas.\n")),
        ])
    }

    fn link(target: &str, syntax: LinkSyntax) -> NoteLink {
        NoteLink {
            target: target.to_owned(),
            heading: None,
            alias: None,
            embed: false,
            syntax,
            line: 0,
        }
    }

    #[test]
    fn links_resolve_by_stem_alias_path_and_relative_path() {
        let index = vault();
        assert_eq!(
            index.resolve("Plan.md", &link("Architecture", LinkSyntax::Wiki)),
            Some("Docs/Architecture.md")
        );
        assert_eq!(
            index.resolve("Plan.md", &link("arch", LinkSyntax::Wiki)),
            Some("Docs/Architecture.md")
        );
        assert_eq!(
            index.resolve("Plan.md", &link("Docs/Architecture", LinkSyntax::Wiki)),
            Some("Docs/Architecture.md")
        );
        assert_eq!(
            index.resolve(
                "Docs/Architecture.md",
                &link("../Plan.md", LinkSyntax::Markdown)
            ),
            Some("Plan.md")
        );
        assert_eq!(
            index.resolve("Plan.md", &link("../../outside.md", LinkSyntax::Markdown)),
            None
        );
        assert_eq!(
            index.resolve("Plan.md", &link("Missing", LinkSyntax::Wiki)),
            None
        );
        assert_eq!(
            index.resolve("Plan.md", &link("https://x.dev", LinkSyntax::Markdown)),
            None
        );
    }

    #[test]
    fn an_ambiguous_name_prefers_the_linking_folder_then_the_shallowest() {
        let index = vault();
        assert_eq!(
            index.resolve("Plan.md", &link("Ideas", LinkSyntax::Wiki)),
            Some("Ideas.md")
        );
        assert_eq!(
            index.resolve("Archive/Ideas.md", &link("Ideas", LinkSyntax::Wiki)),
            Some("Archive/Ideas.md")
        );
    }

    #[test]
    fn backlinks_list_each_linking_line() {
        let index = vault();
        let back = index.backlinks("Docs/Architecture.md");
        let from: Vec<&str> = back.iter().map(|link| link.from.as_str()).collect();
        assert_eq!(from, ["Ideas.md", "Ideas.md", "Plan.md"]);
        assert_eq!(back[2].text, "See [[Architecture]] and [[Ideas]].");
    }

    #[test]
    fn tags_come_from_the_body_and_the_properties_and_nest() {
        let index = vault();
        assert_eq!(
            index.tags(),
            [
                ("design".to_owned(), 1),
                ("project".to_owned(), 1),
                ("project/krad".to_owned(), 1)
            ]
        );
        assert!(index.has_tag("Ideas.md", "project"));
        assert!(!index.has_tag("Archive/Ideas.md", "project"));
    }

    #[test]
    fn finding_matches_titles_stems_and_aliases() {
        let index = vault();
        let paths: Vec<&str> = index
            .find("arch", 10)
            .iter()
            .map(|note| note.path.as_str())
            .collect();
        assert_eq!(paths, ["Docs/Architecture.md"]);
        let ideas: Vec<&str> = index
            .find("idea", 10)
            .iter()
            .map(|note| note.path.as_str())
            .collect();
        assert_eq!(ideas, ["Archive/Ideas.md", "Ideas.md"]);
    }
}
