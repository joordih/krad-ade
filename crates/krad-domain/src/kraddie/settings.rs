use super::name::KraddieId;
use super::rounds::KraddieRounds;
use super::KraddieWorkspace;
use crate::attention::Switch;
use crate::model::{Effort, ModelId};
use crate::permission_mode::PermissionMode;
use crate::recall::ChatDigests;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KraddieSessions {
    pub sidebar: Switch,
    pub folder: Switch,
}

impl Default for KraddieSessions {
    fn default() -> Self {
        Self {
            sidebar: Switch::On,
            folder: Switch::Off,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KraddieSettings {
    #[serde(with = "effort_name", skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<PermissionMode>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub projects: Vec<KraddieWorkspace>,
    pub sessions: KraddieSessions,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_model: Option<ModelId>,
    #[serde(alias = "heartbeat")]
    pub rounds: KraddieRounds,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub reads: Vec<KraddieId>,
    #[serde(skip_serializing_if = "ChatDigests::is_automatic")]
    pub digests: ChatDigests,
}

impl KraddieSettings {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    #[must_use]
    pub fn exported(&self) -> Self {
        Self {
            projects: Vec::new(),
            reads: Vec::new(),
            ..self.clone()
        }
    }

    pub fn share_from(&mut self, own: &KraddieId, from: KraddieId) -> bool {
        if from == *own || self.reads.contains(&from) {
            return false;
        }
        self.reads.push(from);
        true
    }

    pub fn unshare_from(&mut self, from: &KraddieId) -> bool {
        let before = self.reads.len();
        self.reads.retain(|read| read != from);
        self.reads.len() != before
    }

    pub fn add_project(&mut self, project: KraddieWorkspace) -> bool {
        if self.projects.contains(&project) {
            return false;
        }
        self.projects.push(project);
        true
    }

    #[must_use]
    pub fn shows_in_sidebar(&self) -> bool {
        self.sessions.sidebar.is_on() && !self.sessions.folder.is_on()
    }
}

mod effort_name {
    use crate::model::Effort;
    use serde::{Deserialize, Deserializer, Serializer};

    #[allow(clippy::ref_option, clippy::trivially_copy_pass_by_ref)]
    pub(super) fn serialize<S: Serializer>(
        effort: &Option<Effort>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match effort {
            Some(effort) => serializer.collect_str(effort),
            None => serializer.serialize_none(),
        }
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Effort>, D::Error> {
        let level = Option::<String>::deserialize(deserializer)?;
        Ok(level.and_then(|level| level.parse().ok()))
    }
}

#[cfg(test)]
mod tests {
    use super::super::rounds::QuietSpan;
    use super::*;
    use crate::ids::EnvironmentName;
    use std::path::PathBuf;

    fn project(repo: &str) -> KraddieWorkspace {
        KraddieWorkspace {
            environment: EnvironmentName::new("main".to_owned()),
            repo: PathBuf::from(repo),
        }
    }

    #[test]
    fn settings_round_trip_and_old_files_read_as_defaults() -> serde_json::Result<()> {
        let mut settings = KraddieSettings {
            effort: Some(Effort::ExtraHigh),
            permissions: Some(PermissionMode::Plan),
            session_model: ModelId::parse("claude-sonnet-5-5"),
            ..KraddieSettings::default()
        };
        assert!(settings.add_project(project("/work/api")));
        assert!(!settings.add_project(project("/work/api")));
        settings.sessions.folder = Switch::On;
        settings.rounds.switch = Switch::On;
        settings.rounds.after = QuietSpan::TwoHours;
        let text = serde_json::to_string(&settings)?;
        assert!(text.contains(r#""effort":"xhigh""#), "{text}");
        assert_eq!(serde_json::from_str::<KraddieSettings>(&text)?, settings);
        let old: KraddieSettings = serde_json::from_str("{}")?;
        assert!(old.is_default());
        assert_eq!(old.rounds.switch, Switch::Off);
        assert!(old.shows_in_sidebar());
        let odd: KraddieSettings = serde_json::from_str(r#"{"effort":"ludicrous"}"#)?;
        assert_eq!(odd.effort, None);
        assert_eq!(settings.exported().projects, Vec::new());
        assert!(!settings.shows_in_sidebar());
        Ok(())
    }

    #[test]
    fn reading_another_kraddies_memory_is_one_way_and_never_oneself() -> serde_json::Result<()> {
        let id =
            |text: &str| KraddieId::try_from(text.to_owned()).unwrap_or_else(|_| unreachable!());
        let mut settings = KraddieSettings::default();
        assert!(settings.share_from(&id("a"), id("b")));
        assert!(!settings.share_from(&id("a"), id("b")));
        assert!(!settings.share_from(&id("a"), id("a")));
        settings.digests = ChatDigests::Off;
        let text = serde_json::to_string(&settings)?;
        assert!(text.contains(r#""reads":["b"]"#), "{text}");
        assert!(text.contains(r#""digests":"off""#), "{text}");
        assert_eq!(serde_json::from_str::<KraddieSettings>(&text)?, settings);
        assert_eq!(settings.exported().reads.len(), 0);
        assert!(settings.unshare_from(&id("b")));
        assert!(!settings.unshare_from(&id("b")));
        assert_eq!(KraddieSettings::default().digests, ChatDigests::Automatic);
        Ok(())
    }
}
