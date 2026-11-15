use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::{SkillKey, SkillKind, SkillSettings};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
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

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
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

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SkillPlace {
    pub harness: SkillHarness,
    pub scope: SkillScope,
    pub plugin: Option<String>,
    pub file: PathBuf,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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

#[must_use]
pub fn enabled_count(skills: &[LibrarySkill], settings: &SkillSettings) -> usize {
    skills
        .iter()
        .filter(|skill| settings.is_enabled(&skill.key))
        .count()
}
