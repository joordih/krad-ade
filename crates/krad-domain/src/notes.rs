mod docs;
mod format;
mod graph;
mod index;
mod properties;
mod reading;
mod syntax;
mod tabs;

pub use docs::*;
pub use format::*;
pub use graph::*;
pub use index::*;
pub use properties::*;
pub use reading::*;
pub use syntax::*;
pub use tabs::*;

use crate::model::ModelId;

pub const VAULT_DIR: &str = "docs/notes";
pub const ATTACHMENTS_DIR: &str = "attachments";
pub const NOTE_EXTENSION: &str = "md";
pub const DOCS_SOURCES_FILE: &str = "sources.toml";
pub const DEFAULT_DOCS_FOLDER: &str = "Docs";

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
const TITLE_PREFIX: &str = "# ";

#[must_use]
pub fn note_body_hash(body: &str) -> String {
    let hash = body.trim().bytes().fold(FNV_OFFSET, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME)
    });
    format!("{hash:016x}")
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NoteProvenance {
    pub generated_by: Option<String>,
    pub model: Option<ModelId>,
    pub sources: Vec<String>,
    pub generated_hash: Option<String>,
    pub generated_at: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Note {
    pub properties: Properties,
    pub body: String,
}

impl Note {
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let (properties, body) = Properties::split(text);
        Self {
            properties,
            body: body.to_owned(),
        }
    }

    #[must_use]
    pub fn to_markdown(&self) -> String {
        self.properties.write_before(&self.body)
    }

    #[must_use]
    pub fn provenance(&self) -> NoteProvenance {
        self.properties.provenance()
    }

    #[must_use]
    pub fn is_generated(&self) -> bool {
        self.properties.get(KEY_GENERATED_BY).is_some()
    }

    #[must_use]
    pub fn needs_stamp(&self) -> bool {
        self.is_generated() && self.properties.get(KEY_GENERATED_HASH).is_none()
    }

    pub fn stamp(&mut self, now: u64) {
        self.properties
            .set(KEY_GENERATED_HASH, note_body_hash(&self.body));
        if self.properties.get(KEY_GENERATED_AT).is_none() {
            self.properties.set(KEY_GENERATED_AT, now.to_string());
        }
    }

    #[must_use]
    pub fn is_edited(&self) -> bool {
        self.properties
            .get(KEY_GENERATED_HASH)
            .is_some_and(|hash| hash != note_body_hash(&self.body))
    }

    #[must_use]
    pub fn title(&self, fallback: &str) -> String {
        self.body
            .lines()
            .find_map(|line| line.strip_prefix(TITLE_PREFIX))
            .map(|title| title.trim().to_owned())
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| fallback.to_owned())
    }
}

#[must_use]
pub fn continue_note_prompt(note: &str) -> String {
    format!(
        "Read {note} and the code it documents, then add the next section the note needs at \
         its end. Keep everything already there, its front-matter included, write standard \
         Markdown links ([text](path.md)) to other notes of {VAULT_DIR}, and write only that file."
    )
}

#[must_use]
pub fn title_from_stem(stem: &str) -> String {
    let words = stem.replace(['-', '_'], " ");
    let mut chars = words.trim().chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE: &str = "---\ngenerated_by: Codex\nmodel: gpt-5-codex\naliases: [Arch]\ntags:\n  - design\n  - rust\nsources: [crates/, AGENTS.md]\n---\n\n# Architecture\n\nThe workspace is **virtual**.\n";

    #[test]
    fn a_note_keeps_every_property_and_its_body() {
        let note = Note::parse(NOTE);
        assert_eq!(note.to_markdown(), NOTE, "a note round-trips as written");
        let provenance = note.provenance();
        assert_eq!(provenance.generated_by.as_deref(), Some("Codex"));
        assert_eq!(provenance.sources, ["crates/", "AGENTS.md"]);
        assert_eq!(note.title("fallback"), "Architecture");
        assert_eq!(note.properties.list("tags"), ["design", "rust"]);
        assert_eq!(note.properties.list("aliases"), ["Arch"]);
    }

    #[test]
    fn a_note_without_properties_is_all_body() {
        let note = Note::parse("# Mine\n\nText\n");
        assert!(note.properties.is_empty());
        assert_eq!(note.to_markdown(), "# Mine\n\nText\n");
        assert!(!note.is_edited());
        assert_eq!(title_from_stem("dependency-rules"), "Dependency rules");
    }
}
