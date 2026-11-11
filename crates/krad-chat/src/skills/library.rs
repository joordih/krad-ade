mod jsonc;
mod places;

use krad_domain::{SkillKey, SkillKind, SkillSettings};
use krad_host::HostLink;
use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use places::found_documents;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum SkillHarness {
    ClaudeCode,
    Codex,
    Opencode,
    Shared,
}

impl SkillHarness {
    pub const ALL: [Self; 4] = [Self::ClaudeCode, Self::Codex, Self::Opencode, Self::Shared];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::Codex => "Codex",
            Self::Opencode => "opencode",
            Self::Shared => "Shared",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum SkillScope {
    Project,
    Global,
}

impl SkillScope {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Project => "Project",
            Self::Global => "Global",
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SkillPlace {
    pub harness: SkillHarness,
    pub scope: SkillScope,
    pub plugin: Option<String>,
    pub file: PathBuf,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LibrarySkill {
    pub key: SkillKey,
    pub description: String,
    pub places: Vec<SkillPlace>,
    pub clash: bool,
}

impl LibrarySkill {
    #[must_use]
    pub fn harnesses(&self) -> Vec<SkillHarness> {
        let mut harnesses: Vec<SkillHarness> =
            self.places.iter().map(|place| place.harness).collect();
        harnesses.sort();
        harnesses.dedup();
        harnesses
    }

    #[must_use]
    pub fn scopes(&self) -> Vec<SkillScope> {
        let mut scopes: Vec<SkillScope> = self.places.iter().map(|place| place.scope).collect();
        scopes.sort();
        scopes.dedup();
        scopes
    }

    #[must_use]
    pub fn matches(&self, filter: &LibraryFilter) -> bool {
        let query = filter.query.trim().to_lowercase();
        filter.kind.is_none_or(|kind| self.key.kind == kind)
            && filter
                .harness
                .is_none_or(|harness| self.places.iter().any(|place| place.harness == harness))
            && (query.is_empty()
                || self.key.name.to_lowercase().contains(&query)
                || self.description.to_lowercase().contains(&query))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LibraryFilter {
    pub query: String,
    pub kind: Option<SkillKind>,
    pub harness: Option<SkillHarness>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Document {
    key: SkillKey,
    description: String,
    place: SkillPlace,
    fingerprint: u64,
}

fn fingerprint(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.trim().hash(&mut hasher);
    hasher.finish()
}

#[must_use]
#[tracing::instrument(skip_all, fields(project = project.is_some()))]
pub fn skill_library(
    host: &HostLink,
    home: Option<&Path>,
    project: Option<&Path>,
) -> Vec<LibrarySkill> {
    merged(found_documents(host, home, project))
}

fn merged(documents: Vec<Document>) -> Vec<LibrarySkill> {
    let mut by_key: BTreeMap<SkillKey, Vec<(u64, LibrarySkill)>> = BTreeMap::new();
    for document in documents {
        let variants = by_key.entry(document.key.clone()).or_default();
        match variants
            .iter_mut()
            .find(|(print, _)| *print == document.fingerprint)
        {
            Some((_, skill)) => skill.places.push(document.place),
            None => variants.push((
                document.fingerprint,
                LibrarySkill {
                    key: document.key,
                    description: document.description,
                    places: vec![document.place],
                    clash: false,
                },
            )),
        }
    }
    let mut skills: Vec<LibrarySkill> = by_key
        .into_values()
        .flat_map(|variants| {
            let clash = variants.len() > 1;
            variants.into_iter().map(move |(_, mut skill)| {
                skill.clash = clash;
                skill.places.sort_by(|a, b| {
                    (a.scope, a.harness, &a.file).cmp(&(b.scope, b.harness, &b.file))
                });
                skill
            })
        })
        .collect();
    skills.sort_by_cached_key(|skill| (skill.key.name.to_lowercase(), skill.key.kind));
    skills
}

#[must_use]
pub fn enabled_count(skills: &[LibrarySkill], settings: &SkillSettings) -> usize {
    skills
        .iter()
        .filter(|skill| settings.is_enabled(&skill.key))
        .count()
}

#[cfg(test)]
mod tests;
