use super::NoteProvenance;
use crate::model::ModelId;

pub const KEY_GENERATED_BY: &str = "generated_by";
pub const KEY_MODEL: &str = "model";
pub const KEY_SOURCES: &str = "sources";
pub const KEY_GENERATED_HASH: &str = "generated_hash";
pub const KEY_GENERATED_AT: &str = "generated_at";
pub const KEY_TAGS: &str = "tags";
pub const KEY_ALIASES: &str = "aliases";
pub const PROVENANCE_KEYS: [&str; 5] = [
    KEY_GENERATED_BY,
    KEY_MODEL,
    KEY_SOURCES,
    KEY_GENERATED_HASH,
    KEY_GENERATED_AT,
];

const FENCE: &str = "---";
const KEY_SEPARATOR: char = ':';
const LIST_OPEN: char = '[';
const LIST_CLOSE: char = ']';
const LIST_SEPARATOR: char = ',';
const ITEM_MARK: &str = "- ";
const QUOTES: [char; 2] = ['"', '\''];
const NEWLINE: char = '\n';

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Property {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Properties {
    entries: Vec<Property>,
}

fn unquote(value: &str) -> &str {
    value.trim().trim_matches(QUOTES)
}

impl Properties {
    #[must_use]
    pub fn split(text: &str) -> (Self, &str) {
        let Some(rest) = text
            .strip_prefix(FENCE)
            .and_then(|rest| rest.strip_prefix(NEWLINE))
        else {
            return (Self::default(), text);
        };
        let mut offset = 0;
        for line in rest.split_inclusive(NEWLINE) {
            if line.trim_end_matches(['\r', NEWLINE]) == FENCE {
                let body = &rest[offset + line.len()..];
                return (Self::parse(&rest[..offset]), body);
            }
            offset += line.len();
        }
        (Self::default(), text)
    }

    fn parse(front: &str) -> Self {
        let mut entries: Vec<Property> = Vec::new();
        for line in front.lines() {
            let continues = line.starts_with([' ', '\t']) || line.starts_with(ITEM_MARK);
            if let (true, Some(last)) = (continues, entries.last_mut()) {
                last.value.push(NEWLINE);
                last.value.push_str(line);
                continue;
            }
            match line.split_once(KEY_SEPARATOR) {
                Some((key, value)) => entries.push(Property {
                    key: key.trim().to_owned(),
                    value: value.trim_start().to_owned(),
                }),
                None if !line.trim().is_empty() => entries.push(Property {
                    key: line.trim().to_owned(),
                    value: String::new(),
                }),
                None => {}
            }
        }
        Self { entries }
    }

    #[must_use]
    pub fn write_before(&self, body: &str) -> String {
        if self.entries.is_empty() {
            return body.to_owned();
        }
        let lines: Vec<String> = self
            .entries
            .iter()
            .map(|entry| {
                if entry.value.is_empty() {
                    format!("{}{KEY_SEPARATOR}", entry.key)
                } else if entry.value.starts_with(NEWLINE) {
                    format!("{}{KEY_SEPARATOR}{}", entry.key, entry.value)
                } else {
                    format!("{}{KEY_SEPARATOR} {}", entry.key, entry.value)
                }
            })
            .collect();
        format!("{FENCE}\n{}\n{FENCE}\n{body}", lines.join("\n"))
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[must_use]
    pub fn entries(&self) -> &[Property] {
        &self.entries
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|entry| entry.key == key)
            .map(|entry| unquote(&entry.value))
            .filter(|value| !value.is_empty())
    }

    #[must_use]
    pub fn list(&self, key: &str) -> Vec<String> {
        let Some(entry) = self.entries.iter().find(|entry| entry.key == key) else {
            return Vec::new();
        };
        let value = entry.value.trim();
        let items: Vec<&str> = if let Some(inline) = value
            .strip_prefix(LIST_OPEN)
            .and_then(|inner| inner.strip_suffix(LIST_CLOSE))
        {
            inline.split(LIST_SEPARATOR).collect()
        } else if value.contains(NEWLINE) || value.starts_with(ITEM_MARK) {
            value
                .lines()
                .filter_map(|line| line.trim().strip_prefix(ITEM_MARK.trim_end()))
                .collect()
        } else {
            vec![value]
        };
        items
            .into_iter()
            .map(|item| unquote(item).to_owned())
            .filter(|item| !item.is_empty())
            .collect()
    }

    pub fn set(&mut self, key: &str, value: impl Into<String>) {
        let value = value.into();
        match self.entries.iter_mut().find(|entry| entry.key == key) {
            Some(entry) => entry.value = value,
            None => self.entries.push(Property {
                key: key.to_owned(),
                value,
            }),
        }
    }

    pub fn rename(&mut self, key: &str, new_key: &str) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.key == key) {
            new_key.trim().clone_into(&mut entry.key);
        }
    }

    pub fn remove(&mut self, key: &str) {
        self.entries.retain(|entry| entry.key != key);
    }

    pub(super) fn provenance(&self) -> NoteProvenance {
        NoteProvenance {
            generated_by: self.get(KEY_GENERATED_BY).map(str::to_owned),
            model: self.get(KEY_MODEL).and_then(ModelId::parse),
            sources: self.list(KEY_SOURCES),
            generated_hash: self.get(KEY_GENERATED_HASH).map(str::to_owned),
            generated_at: self.get(KEY_GENERATED_AT).and_then(|at| at.parse().ok()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_keys_and_lists_round_trip_in_order() {
        let text = "---\ntitle: \"Plan\"\ndate: 2026-10-29\ncssclasses:\n  - wide\n---\nBody\n";
        let (properties, body) = Properties::split(text);
        assert_eq!(body, "Body\n");
        assert_eq!(properties.write_before(body), text);
        assert_eq!(properties.get("title"), Some("Plan"));
        assert_eq!(properties.list("cssclasses"), ["wide"]);
        let keys: Vec<&str> = properties
            .entries()
            .iter()
            .map(|entry| entry.key.as_str())
            .collect();
        assert_eq!(keys, ["title", "date", "cssclasses"]);
    }

    #[test]
    fn text_without_a_closed_front_matter_is_all_body() {
        let (properties, body) = Properties::split("---\nnot closed\n");
        assert!(properties.is_empty());
        assert_eq!(body, "---\nnot closed\n");
        let (properties, body) = Properties::split("# Title\n---\n");
        assert!(properties.is_empty());
        assert_eq!(body, "# Title\n---\n");
    }

    #[test]
    fn setting_renaming_and_removing_keep_the_other_keys() {
        let (mut properties, body) = Properties::split("Body\n");
        properties.set(KEY_TAGS, "[a, b]");
        properties.set("status", "draft");
        properties.rename("status", "state");
        assert_eq!(
            properties.write_before(body),
            "---\ntags: [a, b]\nstate: draft\n---\nBody\n"
        );
        properties.remove(KEY_TAGS);
        assert_eq!(properties.list(KEY_TAGS), Vec::<String>::new());
        assert_eq!(properties.get("state"), Some("draft"));
    }
}
