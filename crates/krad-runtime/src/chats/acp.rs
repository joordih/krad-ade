use krad_chat::{AcpConfig, AcpEvent, AcpWanted, ChatBackend, LiveChat, ModelOffer};
use krad_domain::{AcpConfigOptionId, AcpConfigValue, AcpModeId};
use krad_protocol::{AcpShown, AcpSignInAsk, AgentRef};
use krad_store::{read_acp_secrets, AcpSecrets};

use super::backend::{ChatLaunch, Unready};
use crate::jobs::{JobDone, Jobs};

const SECRETS_JOB_FAILED: &str = "the agent's secrets could not be read";

#[derive(Debug)]
pub(crate) struct AcpSecretsRead {
    pub(crate) chat: AgentRef,
    pub(crate) opened: u64,
    pub(crate) read: Result<AcpSecrets, String>,
}

#[derive(Debug, Default)]
enum Secrets {
    #[default]
    Unread,
    Reading,
    Read(Result<AcpSecrets, String>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum AcpChoice {
    Mode(AcpModeId),
    Option(AcpConfigOptionId, AcpConfigValue),
}

#[derive(Debug, Default)]
pub(super) struct AcpChat {
    secrets: Secrets,
    pub(super) shown: AcpShown,
    wanted: AcpWanted,
}

impl AcpChat {
    pub(super) fn of(launch: &ChatLaunch) -> Option<Self> {
        let ChatLaunch::Acp(config) = launch else {
            return None;
        };
        Some(Self {
            wanted: config.wanted.clone(),
            ..Self::default()
        })
    }

    pub(super) fn prepared(
        &mut self,
        mut config: AcpConfig,
        (chat, opened): (&AgentRef, u64),
        jobs: &Jobs,
    ) -> Result<ChatBackend, Unready> {
        match &self.secrets {
            Secrets::Read(Ok(secrets)) => {
                config.agent.secrets.clone_from(secrets);
                config.wanted = self.wanted.clone();
                Ok(ChatBackend::Acp(config))
            }
            Secrets::Read(Err(problem)) => Err(Unready::Problem(problem.clone())),
            Secrets::Reading => Err(Unready::Waiting),
            Secrets::Unread => {
                let agent = config.agent.agent;
                let chat = chat.clone();
                let started = jobs.run(move || {
                    JobDone::AcpSecretsRead(AcpSecretsRead {
                        chat,
                        opened,
                        read: read_acp_secrets(&agent).map_err(|error| error.to_string()),
                    })
                });
                self.secrets = match started {
                    Ok(()) => Secrets::Reading,
                    Err(error) => Secrets::Read(Err(format!("{SECRETS_JOB_FAILED}: {error}"))),
                };
                Err(Unready::Waiting)
            }
        }
    }

    pub(super) fn secrets_read(&mut self, read: Result<AcpSecrets, String>) {
        self.secrets = Secrets::Read(read);
    }

    pub(super) fn reported(&mut self, event: AcpEvent) -> Vec<AcpChoice> {
        let mut early = Vec::new();
        if !matches!(event, AcpEvent::SignInNeeded(_)) && !self.shown.reported {
            self.shown.reported = true;
            let wanted = std::mem::take(&mut self.wanted);
            early.extend(wanted.mode.map(AcpChoice::Mode));
            early.extend(
                wanted
                    .options
                    .into_iter()
                    .map(|(option, value)| AcpChoice::Option(option, value)),
            );
        }
        match event {
            AcpEvent::Modes(modes) => self.shown.modes = Some(modes),
            AcpEvent::Models(models) => self.shown.models = Some(models),
            AcpEvent::Options(options) => self.shown.options = options,
            AcpEvent::SignInNeeded(methods) => {
                let asked = self.shown.sign_in.as_ref().map_or(0, |ask| ask.asked);
                self.shown.sign_in = Some(AcpSignInAsk {
                    methods,
                    asked: asked + 1,
                });
            }
        }
        early
    }

    pub(super) fn opened(&mut self) {
        self.shown.sign_in = None;
    }

    pub(super) fn models(&self) -> Vec<ModelOffer> {
        self.shown
            .models
            .iter()
            .flat_map(|models| models.available.iter().map(ModelOffer::from))
            .collect()
    }

    pub(super) fn chose(&mut self, choice: &AcpChoice) {
        match choice {
            AcpChoice::Mode(mode) => {
                if let Some(modes) = self.shown.modes.as_mut() {
                    modes.current = mode.clone();
                }
                if !self.shown.reported {
                    self.wanted.mode = Some(mode.clone());
                }
            }
            AcpChoice::Option(id, value) => {
                if let Some(option) = self
                    .shown
                    .options
                    .iter_mut()
                    .find(|option| option.id == *id)
                {
                    option.current = value.clone();
                }
                if !self.shown.reported {
                    self.wanted.options.retain(|(chosen, _)| chosen != id);
                    self.wanted.options.push((id.clone(), value.clone()));
                }
            }
        }
    }
}

pub(super) fn tell(live: &LiveChat, choice: AcpChoice) {
    match choice {
        AcpChoice::Mode(mode) => live.set_acp_mode(mode),
        AcpChoice::Option(option, value) => live.set_acp_option(option, value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{AcpMode, AcpModes};

    fn modes(current: &str) -> AcpModes {
        AcpModes {
            current: AcpModeId(current.to_owned()),
            available: ["ask", "code"]
                .into_iter()
                .map(|id| AcpMode {
                    id: AcpModeId(id.to_owned()),
                    name: id.to_owned(),
                    description: None,
                })
                .collect(),
        }
    }

    #[test]
    fn choices_made_before_the_agent_reports_are_wanted_then_resent_once() {
        let mut acp = AcpChat::default();
        let effort = AcpConfigOptionId("effort".to_owned());
        acp.chose(&AcpChoice::Mode(AcpModeId("code".to_owned())));
        acp.chose(&AcpChoice::Option(
            effort.clone(),
            AcpConfigValue("low".to_owned()),
        ));
        acp.chose(&AcpChoice::Option(
            effort.clone(),
            AcpConfigValue("high".to_owned()),
        ));
        assert_eq!(acp.reported(AcpEvent::SignInNeeded(Vec::new())), Vec::new());
        assert_eq!(acp.shown.sign_in.as_ref().map(|ask| ask.asked), Some(1));
        assert_eq!(
            acp.reported(AcpEvent::Modes(modes("ask"))),
            [
                AcpChoice::Mode(AcpModeId("code".to_owned())),
                AcpChoice::Option(effort, AcpConfigValue("high".to_owned())),
            ]
        );
        assert_eq!(acp.reported(AcpEvent::Modes(modes("ask"))), Vec::new());
        acp.chose(&AcpChoice::Mode(AcpModeId("code".to_owned())));
        assert_eq!(acp.wanted, AcpWanted::default());
        assert_eq!(
            acp.shown.modes.as_ref().map(|modes| modes.current.clone()),
            Some(AcpModeId("code".to_owned()))
        );
        acp.opened();
        assert_eq!(acp.shown.sign_in, None);
    }
}
