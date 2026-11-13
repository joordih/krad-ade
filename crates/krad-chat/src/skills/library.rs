mod jsonc;
mod places;

use krad_domain::SkillKey;

pub use krad_domain::{
    enabled_count, LibraryFilter, LibrarySkill, SkillHarness, SkillPlace, SkillScope,
};
use krad_host::HostLink;
use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::path::Path;

use places::found_documents;

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

#[cfg(test)]
mod tests;
