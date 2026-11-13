use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::SkillSettings;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub source: SkillSource,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "from", content = "file", rename_all = "snake_case")]
pub enum SkillSource {
    Claude,
    Shared(PathBuf),
}

#[must_use]
pub fn search_skills<'a>(
    skills: &'a [Skill],
    settings: &SkillSettings,
    query: &str,
    limit: usize,
) -> Vec<&'a Skill> {
    let query = query.to_lowercase();
    let mut found: Vec<(bool, &Skill)> = skills
        .iter()
        .filter(|skill| settings.allows_call(&skill.name))
        .filter_map(|skill| {
            let name = skill.name.to_lowercase();
            name.contains(&query)
                .then(|| (!name.starts_with(&query), skill))
        })
        .collect();
    found.sort_by_key(|(later, _)| *later);
    found
        .into_iter()
        .take(limit)
        .map(|(_, skill)| skill)
        .collect()
}
