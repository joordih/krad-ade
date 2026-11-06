mod controls;
mod sign_in;

use krad_chat::AcpEvent;
use krad_domain::{
    AcpConfigOption, AcpConfigOptionId, AcpConfigValue, AcpModeId, AcpModelId, AcpModels, AcpModes,
};

pub(in crate::environment_shell) use sign_in::SignInAsk;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) enum AcpChoice {
    Mode(AcpModeId),
    Model(AcpModelId),
    Option(AcpConfigOptionId, AcpConfigValue),
}

#[derive(Debug, Default)]
pub(in crate::environment_shell) struct AcpOffers {
    modes: Option<AcpModes>,
    models: Option<AcpModels>,
    options: Vec<AcpConfigOption>,
    pub(in crate::environment_shell) sign_in: Option<SignInAsk>,
}

impl AcpOffers {
    pub(super) fn reported(&mut self, event: AcpEvent) {
        match event {
            AcpEvent::Modes(modes) => self.modes = Some(modes),
            AcpEvent::Models(models) => self.models = Some(models),
            AcpEvent::Options(options) => self.options = options,
            AcpEvent::SignInNeeded(methods) => match &mut self.sign_in {
                Some(asked) => asked.asked_again(methods),
                None => self.sign_in = Some(SignInAsk::new(methods)),
            },
        }
    }

    pub(super) fn opened(&mut self) {
        self.sign_in = None;
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
            .filter(|option| !option.choices.is_empty())
    }

    pub(super) fn has_models(&self) -> bool {
        self.models().is_some()
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
            AcpChoice::Model(model) => {
                let Some(models) = self.models.as_mut().filter(|models| models.offers(model))
                else {
                    return false;
                };
                models.current = model.clone();
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
        true
    }
}

#[cfg(test)]
mod tests;
