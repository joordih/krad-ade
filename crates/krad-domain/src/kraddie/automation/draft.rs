use super::super::name::KraddieId;
use super::{
    Automation, AutomationError, AutomationId, AutomationName, AutomationNameError,
    AutomationSession, AutomationTemplate, CatchUp, Trigger,
};
use crate::agent::AgentPreset;
use crate::attention::Switch;
use crate::model::{Effort, ModelId};
use crate::permission_mode::PermissionMode;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DraftError {
    Name(AutomationNameError),
    NoKraddie,
    NoTrigger,
    NoInstructions,
    Automation(AutomationError),
}

impl fmt::Display for DraftError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Name(error) => error.fmt(f),
            Self::NoKraddie => f.write_str("Choose the Kraddie that carries this out"),
            Self::NoTrigger => f.write_str("Give it at least one cue to wake on"),
            Self::NoInstructions => f.write_str("Write a brief so the agent knows the job"),
            Self::Automation(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for DraftError {}

impl From<AutomationNameError> for DraftError {
    fn from(error: AutomationNameError) -> Self {
        Self::Name(error)
    }
}

impl From<AutomationError> for DraftError {
    fn from(error: AutomationError) -> Self {
        Self::Automation(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutomationDraft {
    pub name: String,
    pub switch: Switch,
    pub kraddie: Option<KraddieId>,
    pub triggers: Vec<Trigger>,
    pub instructions: String,
    pub harness: Option<AgentPreset>,
    pub model: Option<ModelId>,
    pub effort: Option<Effort>,
    pub permission_mode: Option<PermissionMode>,
    pub session: AutomationSession,
    pub catch_up: CatchUp,
}

pub const UNTITLED: &str = "Fresh automation";

impl Default for AutomationDraft {
    fn default() -> Self {
        Self {
            name: UNTITLED.to_owned(),
            switch: Switch::On,
            kraddie: None,
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
}

impl AutomationDraft {
    pub fn toggle_trigger(&mut self, trigger: Trigger) {
        if let Some(at) = self.triggers.iter().position(|known| *known == trigger) {
            self.triggers.remove(at);
        } else {
            self.triggers.push(trigger);
        }
    }

    /// # Errors
    /// Fails when the name is not valid, no kraddie owns the automation, it has no trigger or
    /// no instructions, or the automation itself is not valid.
    pub fn validate(&self, id: AutomationId) -> Result<(KraddieId, Automation), DraftError> {
        let name = AutomationName::try_from(self.name.clone())?;
        let kraddie = self.kraddie.clone().ok_or(DraftError::NoKraddie)?;
        if self.triggers.is_empty() {
            return Err(DraftError::NoTrigger);
        }
        let instructions = self.instructions.trim();
        if instructions.is_empty() {
            return Err(DraftError::NoInstructions);
        }
        let automation = Automation {
            id,
            name,
            switch: self.switch,
            triggers: self.triggers.clone(),
            instructions: instructions.to_owned(),
            harness: self.harness,
            model: self.model.clone(),
            effort: self.effort,
            permission_mode: self.permission_mode,
            session: self.session.clone(),
            catch_up: self.catch_up,
        };
        automation.validate()?;
        Ok((kraddie, automation))
    }
}

impl From<(KraddieId, Automation)> for AutomationDraft {
    fn from((kraddie, automation): (KraddieId, Automation)) -> Self {
        Self {
            name: automation.name.into(),
            switch: automation.switch,
            kraddie: Some(kraddie),
            triggers: automation.triggers,
            instructions: automation.instructions,
            harness: automation.harness,
            model: automation.model,
            effort: automation.effort,
            permission_mode: automation.permission_mode,
            session: automation.session,
            catch_up: automation.catch_up,
        }
    }
}

impl From<&AutomationTemplate> for AutomationDraft {
    fn from(template: &AutomationTemplate) -> Self {
        Self {
            name: template.title.clone(),
            triggers: template.triggers.clone(),
            instructions: template.instructions.clone(),
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::session::WorkingCopy;
    use super::*;
    use std::path::PathBuf;

    fn ready() -> AutomationDraft {
        AutomationDraft {
            kraddie: Some(KraddieId::try_from("owl".to_owned()).unwrap_or_else(|_| unreachable!())),
            triggers: vec![Trigger::Manual],
            instructions: "  Look for flaky tests.  ".to_owned(),
            ..AutomationDraft::default()
        }
    }

    #[test]
    fn a_complete_draft_becomes_an_automation_of_its_kraddie() {
        let (kraddie, automation) = ready()
            .validate(AutomationId(3))
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(kraddie.as_str(), "owl");
        assert_eq!(automation.id, AutomationId(3));
        assert_eq!(automation.name.as_str(), UNTITLED);
        assert_eq!(automation.instructions, "Look for flaky tests.");
        let back = AutomationDraft::from((kraddie, automation));
        assert_eq!(back.instructions, "Look for flaky tests.");
    }

    #[test]
    fn every_required_field_is_checked() {
        let id = AutomationId::FIRST;
        let check = |change: fn(&mut AutomationDraft)| {
            let mut draft = ready();
            change(&mut draft);
            draft.validate(id).err()
        };
        assert_eq!(
            check(|draft| draft.name = " ".to_owned()),
            Some(DraftError::Name(AutomationNameError::Empty))
        );
        assert_eq!(
            check(|draft| draft.kraddie = None),
            Some(DraftError::NoKraddie)
        );
        assert_eq!(
            check(|draft| draft.triggers.clear()),
            Some(DraftError::NoTrigger)
        );
        assert_eq!(
            check(|draft| draft.instructions = "\n ".to_owned()),
            Some(DraftError::NoInstructions)
        );
        assert_eq!(
            check(|draft| draft.harness = Some(AgentPreset::Shell)),
            Some(DraftError::Automation(AutomationError::NotAnAgent))
        );
        assert_eq!(
            check(|draft| {
                draft.session.group = Some(PathBuf::from("/work/api"));
                draft.session.working_copy = WorkingCopy::FreshWorktree;
            }),
            Some(DraftError::Automation(AutomationError::GroupNeedsThisRepo))
        );
        assert_eq!(check(|_| {}), None);
    }

    #[test]
    fn toggling_a_trigger_adds_it_once_and_removes_it_again() {
        let mut draft = AutomationDraft::default();
        draft.toggle_trigger(Trigger::Manual);
        assert_eq!(draft.triggers, vec![Trigger::Manual]);
        draft.toggle_trigger(Trigger::Manual);
        assert_eq!(draft.triggers, Vec::new());
    }
}
