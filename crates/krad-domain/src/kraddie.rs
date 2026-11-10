mod activity;
mod automation;
mod compass;
mod design;
mod file;
mod heatmap;
mod history;
mod legacy;
mod limiter;
mod memory;
mod mind;
mod mood;
mod name;
mod name_ideas;
mod rounds;
mod schedule;
mod settings;
mod trigger;

pub use activity::{KraddieActivity, KraddieDate, KraddieDay, ACTIVITY_KEPT_DAYS};
pub use automation::{
    code_host_name, validate_automations, AppTrigger, Automation, AutomationDraft, AutomationError,
    AutomationEvent, AutomationFormatVersion, AutomationId, AutomationKey, AutomationName,
    AutomationNameError, AutomationQuery, AutomationSession, AutomationTemplate, AutomationsFile,
    AutomationsFileError, CatchUp, CodeHostTrigger, CodeHostWake, Conversation, DraftError,
    TemplateCatalog, TemplateCatalogError, TemplateCategory, TemplateIcon, Trigger, WorkingCopy,
    AUTOMATIONS_FORMAT_VERSION, MAX_AUTOMATIONS, MAX_INSTRUCTIONS_CHARS, MAX_TRIGGERS, UNTITLED,
};
pub use compass::{
    CompassTooLong, KraddieCompass, COMPASS_FILE, DEFAULT_COMPASS, LEGACY_COMPASS_FILE,
    MAX_COMPASS_CHARS,
};
pub use heatmap::{
    HeatCell, HeatLevel, HeatMap, HeatMetric, HeatWeek, MonthLabel, DAYS_PER_WEEK, HEATMAP_MONTHS,
    HEAT_LEVELS,
};
pub use memory::{KraddieMemory, MemoryError, MAX_MEMORY_LINES, MAX_MEMORY_LINE_CHARS};
pub use mind::{is_quiet_reply, CheckInContext, KraddieMind, QUIET_REPLY};
pub use rounds::{
    Hush, KraddieRounds, QuietSpan, RoundsCheck, RoundsMoment, RunSlot, QUIET_FIFTEEN_MINUTES,
    QUIET_ONE_HOUR, QUIET_THIRTY_MINUTES, QUIET_TWO_HOURS,
};
pub use settings::{KraddieSessions, KraddieSettings};

pub use design::{
    animation_frame, BodyStyle, CanvasPoint, DesignError, DesignHistory, KraddieAnimation,
    KraddieColor, KraddieColorError, KraddieDesign, KraddieLayer, MoodLook, MoodLooks, PaintTarget,
    PaintTool, PaletteIndex, PixelGrid, PixelGridError, Pose, ANIMATION_FRAME, CANVAS_SIZE,
    MAX_LAYERS, MAX_PALETTE, MAX_UNDO,
};
pub use file::{
    KraddieFile, KraddieFileError, KraddieFormatVersion, ReadKraddie, KRADDIE_FILE_EXTENSION,
    KRADDIE_FORMAT_VERSION, LEGACY_KRADDIE_FILE_EXTENSION,
};
pub use history::{
    KraddieHistory, KraddieOutcome, KraddieRun, KraddieRunId, KraddieSessionLink,
    MAX_KRADDIE_HISTORY,
};
pub use limiter::{KraddieRunLimiter, RunRefusal, MAX_KRADDIE_RUNS};
pub use mood::KraddieMood;
pub use name::{KraddieId, KraddieName, KraddieNameError};
pub use name_ideas::{name_ideas, KRADDIE_NAME_IDEAS, NAME_IDEAS_SHOWN};
pub use schedule::{IntervalError, IntervalMinutes, KraddieSchedule, Weekdays, WeekdaysError};
pub use trigger::{KraddieEvent, KraddieWake};

use crate::agent::AgentPreset;
use crate::attention::Switch;
use crate::ids::EnvironmentName;
use crate::model::ModelId;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KraddieWorkspace {
    pub environment: EnvironmentName,
    pub repo: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Kraddie {
    pub name: KraddieName,
    pub design: KraddieDesign,
    pub harness: AgentPreset,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<KraddieWorkspace>,
    pub switch: Switch,
    #[serde(default, skip_serializing_if = "KraddieSettings::is_default")]
    pub settings: KraddieSettings,
}

impl Kraddie {
    #[must_use]
    pub fn new(name: KraddieName, harness: AgentPreset) -> Self {
        Self {
            name,
            design: KraddieDesign::kraddie(BodyStyle::default()),
            harness,
            model: None,
            workspace: None,
            switch: Switch::On,
            settings: KraddieSettings::default(),
        }
    }

    #[must_use]
    pub fn id(&self) -> KraddieId {
        self.name.id()
    }

    #[must_use]
    pub fn works_in(&self, place: &KraddieWorkspace) -> bool {
        self.switch.is_on() && self.workspace.as_ref() == Some(place)
    }

    #[must_use]
    pub fn exported(&self) -> Self {
        Self {
            workspace: None,
            settings: self.settings.exported(),
            ..self.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(environment: &str, repo: &str) -> KraddieWorkspace {
        KraddieWorkspace {
            environment: EnvironmentName::new(environment.to_owned()),
            repo: PathBuf::from(repo),
        }
    }

    #[test]
    fn a_kraddie_works_only_in_its_own_repository_while_on() {
        let name = "Watcher".parse().unwrap_or_else(|_| unreachable!());
        let mut kraddie = Kraddie::new(name, AgentPreset::ClaudeCode);
        let api = place("main", "/work/api");
        assert!(!kraddie.works_in(&api));
        kraddie.workspace = Some(api.clone());
        assert!(kraddie.works_in(&api));
        assert!(!kraddie.works_in(&place("main", "/work/web")));
        assert!(!kraddie.works_in(&place("remote", "/work/api")));
        kraddie.switch = Switch::Off;
        assert!(!kraddie.works_in(&api));
    }

    #[test]
    fn a_kraddie_file_from_before_the_profile_reads_with_default_settings() {
        let name = "Old".parse().unwrap_or_else(|_| unreachable!());
        let kraddie = Kraddie::new(name, AgentPreset::Codex);
        let text = KraddieFile::new(kraddie.clone(), Vec::new())
            .write()
            .unwrap_or_else(|_| unreachable!());
        assert!(!text.contains("\"settings\""), "{text}");
        let read = KraddieFile::read(&text).unwrap_or_else(|_| unreachable!());
        assert_eq!(read.kraddie.settings, KraddieSettings::default());
        assert_eq!(read.kraddie, kraddie);
    }
}
