use krad_domain::{
    humanize_model_id, AcpAgentStart, AcpConfigOption, AcpModel, AcpModes, ChatEvent,
};
use std::path::Path;
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::acp::{AcpConfig, AcpWanted, OpenBuffers};
use crate::control::{AcpEvent, LiveEvent, ModelOffer};
use crate::error::ChatError;
use crate::live::{new_session_id, start_chat, ChatBackend, LiveConfig};
use crate::live::{BusVars, LiveSession};
use crate::process::Runner;

const CATALOG_WAIT: Duration = Duration::from_secs(30);
const ACP_LISTING_WAIT: Duration = Duration::from_mins(2);
const ACP_SIGN_IN_TEXT: &str = "sign in to the agent in a chat first, then its models show here";
const ACP_SILENT_TEXT: &str = "the agent did not answer in time";

/// # Errors
///
/// Fails when Claude Code cannot be started.
pub fn claude_models(runner: &Runner, cwd: &Path) -> Result<Vec<ModelOffer>, ChatError> {
    let config = LiveConfig {
        runner: runner.clone(),
        model: None,
        cwd: cwd.to_path_buf(),
        system_prompt: None,
        effort: None,
        permission_mode: None,
        bus_vars: BusVars::default(),
        usage_budget: Arc::default(),
    };
    let (sender, receiver) = channel();
    let chat = start_chat(
        ChatBackend::Claude(config),
        &LiveSession::Start(new_session_id()),
        move |event| {
            if let LiveEvent::Ready(catalog) = event {
                sender.send(catalog.models).unwrap_or_default();
            }
        },
    )?;
    let models = receiver.recv_timeout(CATALOG_WAIT).unwrap_or_default();
    chat.stop();
    Ok(models)
}

impl From<&AcpModel> for ModelOffer {
    fn from(model: &AcpModel) -> Self {
        let name = Some(model.name.trim())
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .or_else(|| humanize_model_id(&model.id.0))
            .unwrap_or_else(|| model.id.0.clone());
        Self {
            value: model.id.0.clone(),
            name,
            description: model.description.clone(),
            ..Self::default()
        }
    }
}

struct NoBuffers;

impl OpenBuffers for NoBuffers {
    fn unsaved_text(&self, _: &Path) -> Option<String> {
        None
    }

    fn written(&self, _: &Path, _: &str) {}
}

#[derive(Debug)]
enum AcpListing {
    Listed(Vec<ModelOffer>),
    Failed(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AcpOffered {
    pub models: Vec<ModelOffer>,
    pub modes: Option<AcpModes>,
    pub options: Vec<AcpConfigOption>,
}

#[derive(Debug)]
enum Heard {
    Modes(AcpModes),
    Options(Vec<AcpConfigOption>),
    Ended(AcpListing),
}

fn heard(event: LiveEvent) -> Option<Heard> {
    match event {
        LiveEvent::Acp(AcpEvent::Modes(modes)) => Some(Heard::Modes(modes)),
        LiveEvent::Acp(AcpEvent::Options(options)) => Some(Heard::Options(options)),
        event => acp_listing(event).map(Heard::Ended),
    }
}

fn acp_listing(event: LiveEvent) -> Option<AcpListing> {
    match event {
        LiveEvent::Acp(AcpEvent::Models(models)) => Some(AcpListing::Listed(
            models.available.iter().map(ModelOffer::from).collect(),
        )),
        LiveEvent::Ready(_) => Some(AcpListing::Listed(Vec::new())),
        LiveEvent::Acp(AcpEvent::SignInNeeded(_)) => {
            Some(AcpListing::Failed(ACP_SIGN_IN_TEXT.to_owned()))
        }
        LiveEvent::Chat(ChatEvent::Error(text)) => Some(AcpListing::Failed(text)),
        _ => None,
    }
}

/// # Errors
///
/// Fails when the agent cannot be started, asks to sign in or reports an error
/// before it opens a session.
#[tracing::instrument(skip_all, fields(agent = %agent.agent.id))]
pub fn acp_offers(
    runner: &Runner,
    cwd: &Path,
    agent: AcpAgentStart,
) -> Result<AcpOffered, ChatError> {
    let config = AcpConfig {
        runner: runner.clone(),
        agent,
        cwd: cwd.to_path_buf(),
        mcp: Vec::new(),
        bus_vars: BusVars::default(),
        buffers: Arc::new(NoBuffers),
        model: None,
        wanted: AcpWanted::default(),
    };
    let (sender, receiver) = channel();
    let chat = start_chat(
        ChatBackend::Acp(config),
        &LiveSession::Start(new_session_id()),
        move |event| {
            if let Some(heard) = heard(event) {
                sender.send(heard).unwrap_or_default();
            }
        },
    )?;
    let deadline = Instant::now() + ACP_LISTING_WAIT;
    let mut offered = AcpOffered::default();
    let ended = loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match receiver.recv_timeout(left) {
            Ok(Heard::Modes(modes)) => offered.modes = Some(modes),
            Ok(Heard::Options(options)) => offered.options = options,
            Ok(Heard::Ended(listing)) => break Some(listing),
            Err(_) => break None,
        }
    };
    chat.stop();
    match ended {
        Some(AcpListing::Listed(models)) => Ok(AcpOffered { models, ..offered }),
        Some(AcpListing::Failed(text)) => Err(ChatError::Agent(text)),
        None => Err(ChatError::Agent(ACP_SILENT_TEXT.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{AcpModelId, AcpModels};

    fn model(id: &str, name: &str) -> AcpModel {
        AcpModel {
            id: AcpModelId(id.to_owned()),
            name: name.to_owned(),
            description: Some(format!("about {id}")),
        }
    }

    #[test]
    fn the_models_an_acp_agent_reports_become_offers_in_its_order() {
        let models = AcpModels {
            current: AcpModelId("default".to_owned()),
            available: vec![
                model("default", "Default (recommended)"),
                model("opus", "Opus"),
            ],
        };
        let listed = acp_listing(LiveEvent::Acp(AcpEvent::Models(models)));
        let Some(AcpListing::Listed(offers)) = listed else {
            unreachable!("the models are listed: {listed:?}");
        };
        let shown: Vec<(&str, &str, Option<&str>)> = offers
            .iter()
            .map(|offer| {
                (
                    offer.value.as_str(),
                    offer.name.as_str(),
                    offer.description.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            shown,
            [
                ("default", "Default (recommended)", Some("about default")),
                ("opus", "Opus", Some("about opus")),
            ]
        );
    }

    #[test]
    fn an_unnamed_acp_model_shows_its_id() {
        let shown = humanize_model_id("x-1").unwrap_or_else(|| "x-1".to_owned());
        assert_eq!(ModelOffer::from(&model("x-1", " ")).name, shown);
    }

    #[test]
    fn a_sign_in_or_an_error_ends_the_listing_with_its_reason() {
        assert!(matches!(
            acp_listing(LiveEvent::Acp(AcpEvent::SignInNeeded(Vec::new()))),
            Some(AcpListing::Failed(text)) if text == ACP_SIGN_IN_TEXT
        ));
        assert!(matches!(
            acp_listing(LiveEvent::Chat(ChatEvent::Error("cannot run".to_owned()))),
            Some(AcpListing::Failed(text)) if text == "cannot run"
        ));
        assert!(acp_listing(LiveEvent::Notice("hi".to_owned())).is_none());
    }

    #[test]
    fn the_probe_keeps_the_modes_and_options_it_hears_before_the_models_end_it() {
        let modes = AcpModes {
            current: krad_domain::AcpModeId("default".to_owned()),
            available: Vec::new(),
        };
        assert!(matches!(
            heard(LiveEvent::Acp(AcpEvent::Modes(modes.clone()))),
            Some(Heard::Modes(kept)) if kept == modes
        ));
        assert!(matches!(
            heard(LiveEvent::Acp(AcpEvent::Options(Vec::new()))),
            Some(Heard::Options(options)) if options.is_empty()
        ));
        assert!(matches!(
            heard(LiveEvent::Chat(ChatEvent::Error("cannot run".to_owned()))),
            Some(Heard::Ended(AcpListing::Failed(_)))
        ));
    }
}
