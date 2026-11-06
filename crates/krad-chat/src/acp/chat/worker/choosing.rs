use krad_domain::{AcpAuthMethodId, AcpConfigValue, AcpModeId, AcpModelId, ChatEvent};
use tracing::debug;

use super::{Flow, Worker, SIGN_IN_FAILED_TEXT, SIGN_IN_TIMEOUT};
use crate::acp::chat::choices::Setter;
use crate::acp::chat::opening::auth_methods;
use crate::acp::chat::{settle_later, Answered};
use crate::acp::config::SetSessionConfigOptionRequest;
use crate::acp::connection::AcpError;
use crate::acp::ids::{ConfigOptionId, SessionId};
use crate::acp::setup::{AuthenticateRequest, SetSessionModeRequest, SetSessionModelRequest};
use crate::control::{AcpEvent, LiveEvent};

impl<S: FnMut(LiveEvent)> Worker<S> {
    pub(super) fn choose_mode(&mut self, mode: AcpModeId) {
        let Some(session) = self.session.clone() else {
            return;
        };
        let Some(setter) = self
            .choices
            .modes
            .as_ref()
            .map(|modes| modes.setter.clone())
        else {
            debug!("the ACP agent offers no modes");
            return;
        };
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
    }

    pub(super) fn choose_model(&mut self, model: AcpModelId) {
        let Some(session) = self.session.clone() else {
            return;
        };
        let Some(setter) = self
            .choices
            .models
            .as_ref()
            .filter(|models| models.offer.offers(&model))
            .map(|models| models.setter.clone())
        else {
            debug!("the ACP agent does not offer that model");
            return;
        };
        match setter {
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
    }

    pub(super) fn choose_option(&mut self, option: ConfigOptionId, value: AcpConfigValue) {
        let Some(session) = self.session.clone() else {
            return;
        };
        if !self.choices.offers_other(&option, &value) {
            debug!("the ACP agent does not offer that choice");
            return;
        }
        self.configure(session, option, value.0);
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
