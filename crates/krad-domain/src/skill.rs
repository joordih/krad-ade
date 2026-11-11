use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillKind {
    Skill,
    Agent,
    Command,
}

impl SkillKind {
    pub const ALL: [Self; 3] = [Self::Skill, Self::Agent, Self::Command];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Skill => "Skill",
            Self::Agent => "Agent",
            Self::Command => "Command",
        }
    }

    #[must_use]
    pub const fn is_callable(self) -> bool {
        matches!(self, Self::Skill | Self::Command)
    }
}

impl fmt::Display for SkillKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub struct SkillKey {
    pub kind: SkillKind,
    pub name: String,
}

impl SkillKey {
    #[must_use]
    pub fn new(kind: SkillKind, name: &str) -> Self {
        Self {
            kind,
            name: name.to_owned(),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct SkillSettings {
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub disabled: BTreeSet<SkillKey>,
}

impl SkillSettings {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.disabled.is_empty()
    }

    #[must_use]
    pub fn is_enabled(&self, key: &SkillKey) -> bool {
        !self.disabled.contains(key)
    }

    #[must_use]
    pub fn allows_call(&self, name: &str) -> bool {
        SkillKind::ALL
            .into_iter()
            .filter(|kind| kind.is_callable())
            .all(|kind| self.is_enabled(&SkillKey::new(kind, name)))
    }

    pub fn set_enabled(&mut self, key: SkillKey, enabled: bool) {
        if enabled {
            self.disabled.remove(&key);
        } else {
            self.disabled.insert(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_disabled_skill_is_kept_by_kind_and_name() {
        let mut settings = SkillSettings::default();
        let review = SkillKey::new(SkillKind::Skill, "review");
        settings.set_enabled(review.clone(), false);
        assert!(!settings.is_enabled(&review));
        assert!(settings.is_enabled(&SkillKey::new(SkillKind::Agent, "review")));
        settings.set_enabled(review.clone(), true);
        assert!(settings.is_empty());
    }

    #[test]
    fn a_name_cannot_be_called_while_a_callable_kind_of_it_is_off() {
        let mut settings = SkillSettings::default();
        settings.set_enabled(SkillKey::new(SkillKind::Agent, "planner"), false);
        assert!(settings.allows_call("planner"));
        settings.set_enabled(SkillKey::new(SkillKind::Command, "deploy"), false);
        assert!(!settings.allows_call("deploy"));
    }

    #[test]
    fn the_disabled_set_round_trips_through_json() -> Result<(), serde_json::Error> {
        let mut settings = SkillSettings::default();
        settings.set_enabled(SkillKey::new(SkillKind::Skill, "lint"), false);
        let text = serde_json::to_string(&settings)?;
        assert_eq!(text, r#"{"disabled":[{"kind":"skill","name":"lint"}]}"#);
        assert_eq!(serde_json::from_str::<SkillSettings>(&text)?, settings);
        assert_eq!(
            serde_json::from_str::<SkillSettings>("{}")?,
            SkillSettings::default()
        );
        Ok(())
    }
}
