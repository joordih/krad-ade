use krad_domain::{AcpAuthMethodId, AcpConfigValue, AcpModeId, AcpModelId, ChatEvent};
use tracing::{debug, info};

use super::{AcpWanted, Flow, Worker, SIGN_IN_FAILED_TEXT, SIGN_IN_TIMEOUT};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ModelWish {
    Preferred,
    Asked,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct WantedModel {
    pub(super) id: AcpModelId,
    pub(super) wish: ModelWish,
}

const NO_MODELS_TEXT: &str =
    "the agent offers no models to choose from, so it keeps its own instead of";
const MODEL_NOT_OFFERED_TEXT: &str = "the agent does not offer the model";
use crate::acp::chat::choices::Setter;
use crate::acp::chat::opening::auth_methods;
use crate::acp::chat::{settle_later, Answered};
use crate::acp::config::SetSessionConfigOptionRequest;
use crate::acp::connection::AcpError;
use crate::acp::ids::{ConfigOptionId, SessionId};
use crate::acp::setup::{AuthenticateRequest, SetSessionModeRequest, SetSessionModelRequest};
use crate::control::{AcpEvent, LiveEvent};

impl<S: FnMut(LiveEvent)> Worker<S> {
    pub(super) fn choose_wanted(&mut self) -> usize {
        let wanted = std::mem::take(&mut self.wanted);
        let mode = wanted.mode.filter(|mode| self.choose_mode(mode.clone()));
        let options: Vec<(ConfigOptionId, AcpConfigValue)> = wanted
            .options
            .into_iter()
            .filter(|(option, value)| self.choose_option(option.clone(), value.clone()))
            .collect();
        self.in_flight = AcpWanted { mode, options };
        self.overlay_in_flight();
        usize::from(self.in_flight.mode.is_some()) + self.in_flight.options.len()
    }

    pub(super) fn overlay_in_flight(&mut self) {
        let choices = &mut self.choices;
        let in_flight = &mut self.in_flight;
        if let Some(mode) = in_flight.mode.clone() {
            choices.mode_is(mode);
        }
        in_flight
            .options
            .retain(|(option, value)| !choices.other_is_already(option, value));
        for (option, value) in &in_flight.options {
            choices.option_is(option, value.clone());
        }
    }

    pub(super) fn choose_mode(&mut self, mode: AcpModeId) -> bool {
        let Some(session) = self.session.clone() else {
            return false;
        };
        let Some(modes) = self.choices.modes.as_ref() else {
            debug!("the ACP agent offers no modes");
            return false;
        };
        if modes.offer.current == mode
            || !modes
                .offer
                .available
                .iter()
                .any(|offered| offered.id == mode)
        {
            debug!("the ACP agent already runs that mode or does not offer it");
            return false;
        }
        let setter = modes.setter.clone();
        match setter {
            Setter::Method => {
                let pending = self.connection.set_mode(&SetSessionModeRequest {
                    session_id: session,
                    mode_id: mode.clone(),
                });
                settle_later(pending, &self.inbound, move |set| {
                    Answered::ModeSet(mode, set)
                });
            }
            Setter::Config(config_id) => self.configure(session, config_id, mode.0),
        }
        true
    }

    pub(super) fn choose_model(&mut self, wanted: WantedModel) -> bool {
        let Some(session) = self.session.clone() else {
            self.wanted_model = Some(wanted);
            return false;
        };
        let Some(models) = self.choices.models.as_ref() else {
            self.unoffered(&wanted, NO_MODELS_TEXT);
            return false;
        };
        let Some(model) = models
            .offer
            .answering(&wanted.id.0)
            .map(|model| model.id.clone())
        else {
            self.unoffered(&wanted, MODEL_NOT_OFFERED_TEXT);
            return false;
        };
        if model == models.offer.current {
            debug!("the ACP agent already runs the chosen model");
            return false;
        }
        match models.setter.clone() {
            Setter::Method => {
                let pending = self.connection.set_model(&SetSessionModelRequest {
                    session_id: session,
                    model_id: model.clone(),
                });
                settle_later(pending, &self.inbound, move |set| {
                    Answered::ModelSet(model, set)
                });
            }
            Setter::Config(config_id) => self.configure(session, config_id, model.0),
        }
        info!("asked the ACP agent for the chosen model");
        true
    }

    fn unoffered(&mut self, wanted: &WantedModel, reason: &str) {
        match wanted.wish {
            ModelWish::Asked => self.notice(format!("{reason} {}", wanted.id.0)),
            ModelWish::Preferred => {
                debug!("the ACP agent does not offer the preferred model, so it keeps its own");
            }
        }
    }

    pub(super) fn choose_option(&mut self, option: ConfigOptionId, value: AcpConfigValue) -> bool {
        let Some(session) = self.session.clone() else {
            return false;
        };
        if !self.choices.offers_other(&option, &value) {
            debug!("the ACP agent does not offer that choice");
            return false;
        }
        if self.choices.other_is_already(&option, &value) {
            debug!("the ACP agent already runs that choice");
            return false;
        }
        self.configure(session, option, value.0);
        true
    }

    fn configure(&self, session: SessionId, config_id: ConfigOptionId, value: String) {
        let pending = self
            .connection
            .set_config_option(&SetSessionConfigOptionRequest {
                session_id: session,
                config_id,
                value,
            });
        settle_later(pending, &self.inbound, Answered::Configured);
    }

    pub(super) fn sign_in(&mut self, method: AcpAuthMethodId) -> Flow {
        let Some(opening) = self.signing_in.take() else {
            debug!("a sign-in came while the ACP agent did not ask for one");
            return Flow::Go;
        };
        let signed = self
            .connection
            .authenticate(&AuthenticateRequest { method_id: method })
            .wait_timeout(SIGN_IN_TIMEOUT);
        match signed {
            Ok(_) => self.open(opening),
            Err(AcpError::Refused(refusal)) => {
                self.notice(format!("{SIGN_IN_FAILED_TEXT}: {refusal}"));
                self.signing_in = Some(opening);
                let methods = auth_methods(&self.methods);
                (self.sink)(LiveEvent::Acp(AcpEvent::SignInNeeded(methods)));
                Flow::Go
            }
            Err(error) => {
                let text = self.failure_text(&error);
                self.emit(ChatEvent::Error(text));
                Flow::Stop
            }
        }
    }
}
