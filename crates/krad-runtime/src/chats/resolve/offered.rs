use krad_domain::{model_label, AgentPreset, ModelId};

use super::super::backend::Choices;
use super::super::ChatPlace;
use super::LaunchFacts;

const UNAVAILABLE_MODEL_NOTICE: &str =
    "is no longer offered here, so this message uses the default model";

pub(super) enum Offered {
    Kept(Option<ModelId>),
    Dropped(String),
}

impl Offered {
    pub(super) fn model(&self) -> Option<ModelId> {
        match self {
            Self::Kept(model) => model.clone(),
            Self::Dropped(_) => None,
        }
    }

    pub(super) fn notice(self) -> Option<String> {
        match self {
            Self::Kept(_) => None,
            Self::Dropped(notice) => Some(notice),
        }
    }
}

impl LaunchFacts<'_> {
    pub(super) fn offered(
        &self,
        harness: AgentPreset,
        place: &ChatPlace,
        choices: &Choices,
    ) -> Offered {
        let Some(model) = choices.model.clone() else {
            return Offered::Kept(None);
        };
        let offers = self.models.offers(&place.host, harness);
        if offers.is_empty() || offers.iter().any(|offer| offer.answers_to(model.as_str())) {
            return Offered::Kept(Some(model));
        }
        Offered::Dropped(format!(
            "{} {UNAVAILABLE_MODEL_NOTICE}",
            model_label(model.as_str())
        ))
    }
}
