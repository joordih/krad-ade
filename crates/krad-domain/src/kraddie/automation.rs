mod brief;
mod draft;
mod file;
mod name;
mod session;
mod template;
mod trigger;

pub use draft::{AutomationDraft, DraftError, UNTITLED};
pub use file::{
    validate_automations, AutomationFormatVersion, AutomationsFile, AutomationsFileError,
    AUTOMATIONS_FORMAT_VERSION,
};
pub use name::{AutomationId, AutomationKey, AutomationName, AutomationNameError, AutomationQuery};
pub use session::{AutomationSession, CatchUp, Conversation, WorkingCopy};
pub use template::{
    AutomationTemplate, TemplateCatalog, TemplateCatalogError, TemplateCategory, TemplateIcon,
};
pub use trigger::{
    code_host_name, AppTrigger, AutomationEvent, CodeHostTrigger, CodeHostWake, Trigger,
};

use super::schedule::KraddieSchedule;
use crate::agent::AgentPreset;
use crate::attention::Switch;
use crate::model::{Effort, ModelId};
use crate::permission_mode::PermissionMode;
use chrono::{DateTime, TimeDelta, TimeZone};
use serde::{Deserialize, Serialize};
use std::fmt;

pub const MAX_INSTRUCTIONS_CHARS: usize = 20_000;
pub const MAX_TRIGGERS: usize = 12;
pub const MAX_AUTOMATIONS: usize = 50;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutomationError {
    InstructionsTooLong,
    RepeatedTrigger,
    TooManyTriggers,
    NotAnAgent,
    GroupNeedsThisRepo,
}

impl fmt::Display for AutomationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InstructionsTooLong => f.write_str("The automation's instructions are too long"),
            Self::RepeatedTrigger => f.write_str("The automation names a trigger twice"),
            Self::TooManyTriggers => {
                write!(f, "An automation has at most {MAX_TRIGGERS} triggers")
            }
            Self::NotAnAgent => f.write_str("The automation's harness is not a coding agent"),
            Self::GroupNeedsThisRepo => {
                f.write_str("A run opens in a sidebar group only when it works in this repo")
            }
        }
    }
}

impl std::error::Error for AutomationError {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Automation {
    pub id: AutomationId,
    pub name: AutomationName,
    pub switch: Switch,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triggers: Vec<Trigger>,
    #[serde(default)]
    pub instructions: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness: Option<AgentPreset>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<PermissionMode>,
    #[serde(default)]
    pub session: AutomationSession,
    #[serde(default)]
    pub catch_up: CatchUp,
}

impl Automation {
    #[must_use]
    pub fn new(id: AutomationId, name: AutomationName) -> Self {
        Self {
            id,
            name,
            switch: Switch::On,
            triggers: Vec::new(),
            instructions: String::new(),
            harness: None,
            model: None,
            effort: None,
            permission_mode: None,
            session: AutomationSession::default(),
            catch_up: CatchUp::default(),
        }
    }

    #[must_use]
    pub fn is_manual(&self) -> bool {
        self.triggers.contains(&Trigger::Manual)
    }

    pub fn schedules(&self) -> impl Iterator<Item = &KraddieSchedule> {
        self.triggers.iter().filter_map(Trigger::schedule)
    }

    #[must_use]
    pub fn listens_for(&self, event: AutomationEvent) -> bool {
        self.switch.is_on() && self.triggers.iter().any(|trigger| trigger.hears(event))
    }

    #[must_use]
    pub fn next_after<Tz: TimeZone>(&self, since: &DateTime<Tz>) -> Option<DateTime<Tz>> {
        self.schedules()
            .filter_map(|schedule| schedule.next_after(since))
            .min()
    }

    #[must_use]
    pub fn is_due<Tz: TimeZone>(&self, since: &DateTime<Tz>, now: &DateTime<Tz>) -> bool {
        self.switch.is_on() && self.next_after(since).is_some_and(|next| next <= *now)
    }

    #[must_use]
    pub fn missed_run<Tz: TimeZone>(
        &self,
        last: Option<&DateTime<Tz>>,
        opened: &DateTime<Tz>,
    ) -> bool {
        if !self.switch.is_on() {
            return false;
        }
        let Some(window) = self
            .catch_up
            .window()
            .and_then(|window| TimeDelta::from_std(window).ok())
        else {
            return false;
        };
        let Some(earliest) = opened.clone().checked_sub_signed(window) else {
            return false;
        };
        let since = last
            .filter(|last| **last > earliest)
            .cloned()
            .unwrap_or(earliest);
        self.next_after(&since).is_some_and(|slot| slot <= *opened)
    }

    #[must_use]
    pub fn exported(&self) -> Self {
        Self {
            session: AutomationSession {
                group: None,
                ..self.session.clone()
            },
            ..self.clone()
        }
    }

    /// # Errors
    /// Fails when the instructions are too long, a trigger repeats, there are too many
    /// triggers, the harness is not an agent or a sidebar group is chosen for a worktree run.
    pub fn validate(&self) -> Result<(), AutomationError> {
        if self.instructions.chars().count() > MAX_INSTRUCTIONS_CHARS {
            return Err(AutomationError::InstructionsTooLong);
        }
        if self.triggers.len() > MAX_TRIGGERS {
            return Err(AutomationError::TooManyTriggers);
        }
        let repeated = self
            .triggers
            .iter()
            .enumerate()
            .any(|(at, trigger)| self.triggers[..at].contains(trigger));
        if repeated {
            return Err(AutomationError::RepeatedTrigger);
        }
        if self.harness.is_some_and(|harness| !harness.is_agent()) {
            return Err(AutomationError::NotAnAgent);
        }
        if self.session.group.is_some() && self.session.working_copy != WorkingCopy::ThisRepo {
            return Err(AutomationError::GroupNeedsThisRepo);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
