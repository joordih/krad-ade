use krad_agents::AgentStatus;
use krad_chat::ModelOffer;
use krad_domain::{AgentPreset, Effort, ModelId, TerminalId};

use super::super::EnvironmentShell;
use super::structured::StructuredChat;
use super::transcript::TRANSCRIPT_HARNESS;
use super::{Backend, SurfaceChat};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::environment_shell) struct AccountUsage {
    pub(in crate::environment_shell) five_hour: Option<u16>,
    pub(in crate::environment_shell) seven_day: Option<u16>,
}

pub(super) struct ModelPick {
    pub(super) id: ModelId,
    pub(super) label: String,
    pub(super) current: bool,
    pub(super) harness: AgentPreset,
}

pub(super) struct Facts {
    pub(super) model: Option<String>,
    pub(super) models: Vec<ModelPick>,
    pub(super) effort: Option<Effort>,
    pub(super) efforts: Vec<Effort>,
    pub(super) thinking: Option<bool>,
    pub(super) context: Option<u16>,
    pub(super) context_window: Option<u64>,
    pub(super) account: Option<String>,
    pub(super) usage: AccountUsage,
}

fn settings_picks(
    models: Vec<ModelId>,
    current: Option<&str>,
    harness: AgentPreset,
) -> Vec<ModelPick> {
    models
        .into_iter()
        .map(|model| ModelPick {
            current: current.is_some_and(|current| current.contains(model.as_str())),
            label: model.as_str().to_owned(),
            id: model,
            harness,
        })
        .collect()
}

fn offer_picks(
    offers: &[ModelOffer],
    current: Option<&str>,
    harness: AgentPreset,
) -> Vec<ModelPick> {
    offers
        .iter()
        .filter_map(|offer| {
            Some(ModelPick {
                id: ModelId::parse(&offer.value)?,
                label: offer.name.clone(),
                current: current == Some(offer.value.as_str()),
                harness,
            })
        })
        .collect()
}

fn transcript_facts(status: AgentStatus, models: Vec<ModelId>, account: AccountUsage) -> Facts {
    Facts {
        models: settings_picks(models, status.model_id.as_deref(), TRANSCRIPT_HARNESS),
        model: status.model_name,
        effort: status
            .effort
            .as_deref()
            .and_then(|level| level.parse().ok()),
        efforts: Effort::ALL.to_vec(),
        thinking: status.thinking,
        context: status.context_used,
        context_window: None,
        account: None,
        usage: AccountUsage {
            five_hour: status.five_hour_used.or(account.five_hour),
            seven_day: status.seven_day_used.or(account.seven_day),
        },
    }
}

struct Offered<'a> {
    fallback: Option<&'a [ModelOffer]>,
    settings: Vec<ModelId>,
    others: Vec<ModelPick>,
}

impl StructuredChat {
    fn facts(&self, offered: Offered<'_>, usage: AccountUsage) -> Facts {
        let offers = self
            .catalog
            .as_ref()
            .map(|catalog| catalog.models.as_slice())
            .or(offered.fallback);
        let chosen = self.chosen_model.as_ref();
        let shown = offers.and_then(|offers| {
            offers
                .iter()
                .find(|offer| chosen.is_some_and(|chosen| chosen.as_str() == offer.value))
                .or_else(|| offers.first())
        });
        let mut models = offers.map_or_else(
            || settings_picks(offered.settings, chosen.map(ModelId::as_str), self.harness),
            |offers| {
                offer_picks(
                    offers,
                    shown.map(|shown| shown.value.as_str()),
                    self.harness,
                )
            },
        );
        models.extend(offered.others);
        let all_efforts = match self.harness {
            AgentPreset::ClaudeCode => Effort::ALL.to_vec(),
            _ => Vec::new(),
        };
        Facts {
            model: shown
                .map(|offer| offer.name.clone())
                .or_else(|| self.model.clone()),
            models,
            effort: self.effort,
            efforts: shown.map_or(all_efforts, |offer| offer.efforts.clone()),
            thinking: shown
                .filter(|offer| offer.thinking_off)
                .map(|_| self.thinking),
            context: self.context_used(),
            context_window: self.context_window,
            account: self
                .catalog
                .as_ref()
                .and_then(|catalog| catalog.subscription.clone()),
            usage,
        }
    }
}

impl EnvironmentShell {
    pub(super) fn composer_facts(&self, id: TerminalId, chat: &SurfaceChat) -> Facts {
        let claude_models = self.settings.models(AgentPreset::ClaudeCode);
        match &chat.backend {
            Backend::Transcript(_) => {
                let status = self
                    .envs
                    .get(&self.active)
                    .and_then(|live| live.statuses.get(&id))
                    .cloned()
                    .unwrap_or_default();
                transcript_facts(status, claude_models, self.account_usage)
            }
            Backend::Structured(structured) => {
                let fresh = chat.messages.is_empty() && structured.is_fresh();
                let opencode = self.opencode_models();
                let offered = match structured.harness {
                    AgentPreset::Opencode => Offered {
                        fallback: Some(opencode),
                        settings: Vec::new(),
                        others: if fresh {
                            settings_picks(claude_models, None, AgentPreset::ClaudeCode)
                        } else {
                            Vec::new()
                        },
                    },
                    _ => Offered {
                        fallback: None,
                        settings: claude_models,
                        others: if fresh {
                            offer_picks(opencode, None, AgentPreset::Opencode)
                        } else {
                            Vec::new()
                        },
                    },
                };
                structured.facts(offered, self.account_usage)
            }
        }
    }
}
