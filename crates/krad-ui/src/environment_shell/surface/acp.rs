mod adapter;
mod controls;
mod sign_in;

use krad_chat::{AcpOffered, AcpWanted};
use krad_domain::{
    AcpConfigOption, AcpConfigOptionId, AcpConfigValue, AcpEffort, AcpModeId, AcpModels, AcpModes,
    Effort, ModelId, ModelOffer,
};
use krad_protocol::AcpShown;

pub(in crate::environment_shell) use adapter::KeptAdapterView;
pub(in crate::environment_shell) use sign_in::SignInAsk;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) enum AcpChoice {
    Mode(AcpModeId),
    Option(AcpConfigOptionId, AcpConfigValue),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::environment_shell) struct AcpSeed {
    modes: Option<AcpModes>,
    options: Vec<AcpConfigOption>,
}

impl From<&AcpOffered> for AcpSeed {
    fn from(offered: &AcpOffered) -> Self {
        Self {
            modes: offered.modes.clone(),
            options: offered.options.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Source {
    #[default]
    Seed,
    Agent,
}

#[derive(Debug, Default)]
pub(in crate::environment_shell) struct AcpOffers {
    modes: Option<AcpModes>,
    models: Option<AcpModels>,
    options: Vec<AcpConfigOption>,
    source: Source,
    chosen_early: Vec<AcpChoice>,
    pub(in crate::environment_shell) sign_in: Option<SignInAsk>,
    sign_in_asked: u32,
}

impl AcpChoice {
    fn sets_the_same_as(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Mode(_), Self::Mode(_)) => true,
            (Self::Option(id, _), Self::Option(other, _)) => id == other,
            (Self::Mode(_), Self::Option(..)) | (Self::Option(..), Self::Mode(_)) => false,
        }
    }
}

impl AcpOffers {
    pub(in crate::environment_shell) fn seed(&mut self, seed: &AcpSeed) {
        if self.source == Source::Agent || !self.chosen_early.is_empty() {
            return;
        }
        self.modes.clone_from(&seed.modes);
        self.options.clone_from(&seed.options);
    }

    pub(in crate::environment_shell) fn wanted(&self) -> AcpWanted {
        let mut wanted = AcpWanted::default();
        for choice in &self.chosen_early {
            match choice {
                AcpChoice::Mode(mode) => wanted.mode = Some(mode.clone()),
                AcpChoice::Option(option, value) => {
                    wanted.options.push((option.clone(), value.clone()));
                }
            }
        }
        wanted
    }

    pub(in crate::environment_shell) fn adopt(&mut self, shown: &AcpShown) -> bool {
        if shown.reported {
            self.source = Source::Agent;
            self.chosen_early.clear();
            self.modes.clone_from(&shown.modes);
            self.models.clone_from(&shown.models);
            self.options.clone_from(&shown.options);
        }
        let Some(asked) = &shown.sign_in else {
            self.sign_in = None;
            return false;
        };
        if asked.asked <= self.sign_in_asked {
            return false;
        }
        self.sign_in_asked = asked.asked;
        match &mut self.sign_in {
            Some(ask) => ask.asked_again(asked.methods.clone()),
            None => self.sign_in = Some(SignInAsk::new(asked.methods.clone())),
        }
        true
    }

    fn modes(&self) -> Option<&AcpModes> {
        self.modes
            .as_ref()
            .filter(|modes| !modes.available.is_empty())
    }

    fn models(&self) -> Option<&AcpModels> {
        self.models
            .as_ref()
            .filter(|models| !models.available.is_empty())
    }

    fn options(&self) -> impl Iterator<Item = &AcpConfigOption> {
        self.options
            .iter()
            .filter(|option| !option.choices.is_empty() && option.shows_as_pill())
    }

    pub(super) fn effort(&self) -> Option<AcpEffort> {
        AcpEffort::of(&self.options)
    }

    pub(super) fn effort_choices(&self, wanted: Effort) -> Vec<AcpChoice> {
        AcpEffort::choose(&self.options, wanted)
            .into_iter()
            .map(|(option, value)| AcpChoice::Option(option, value))
            .collect()
    }

    pub(super) fn fast_choice(&self, on: bool) -> Option<AcpChoice> {
        AcpEffort::switch_fast(&self.options, on)
            .map(|(option, value)| AcpChoice::Option(option, value))
    }

    pub(super) fn live_models(&self) -> Option<(Vec<ModelOffer>, Option<ModelId>)> {
        let models = self.models()?;
        let offers = models.available.iter().map(ModelOffer::from).collect();
        Some((offers, ModelId::parse(&models.current.0)))
    }

    pub(super) fn chose(&mut self, choice: &AcpChoice) -> bool {
        match choice {
            AcpChoice::Mode(mode) => {
                let Some(modes) = self
                    .modes
                    .as_mut()
                    .filter(|modes| modes.available.iter().any(|offered| &offered.id == mode))
                else {
                    return false;
                };
                modes.current = mode.clone();
            }
            AcpChoice::Option(id, value) => {
                let Some(option) = self
                    .options
                    .iter_mut()
                    .find(|option| &option.id == id && option.offers(value))
                else {
                    return false;
                };
                option.current = value.clone();
            }
        }
        if self.source != Source::Agent {
            self.chosen_early
                .retain(|early| !early.sets_the_same_as(choice));
            self.chosen_early.push(choice.clone());
        }
        true
    }
}

#[cfg(test)]
mod tests;
