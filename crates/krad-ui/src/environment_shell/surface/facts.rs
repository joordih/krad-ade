use krad_agents::AgentStatus;
use krad_chat::{ModelOffer, ModelSource};
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
    pub(super) source: Option<ModelSource>,
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
            source: None,
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
                source: offer.source.clone(),
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

const CHAT_HARNESSES: [AgentPreset; 4] = [
    AgentPreset::ClaudeCode,
    AgentPreset::Opencode,
    AgentPreset::Codex,
    AgentPreset::Gateway,
];

impl EnvironmentShell {
    pub(in crate::environment_shell) fn chats_on(&self, harness: AgentPreset) -> bool {
        match harness {
            AgentPreset::Gateway => self.gateway_ready(),
            other => self.native_agents.contains(&other),
        }
    }

    fn harness_offers(&self, harness: AgentPreset) -> Option<&[ModelOffer]> {
        let offers = match harness {
            AgentPreset::Opencode => self.opencode_models(),
            AgentPreset::Gateway => self.gateway_offers(),
            other => self.harness_models.of(other),
        };
        (!offers.is_empty()).then_some(offers)
    }

    fn harness_picks(&self, harness: AgentPreset) -> Vec<ModelPick> {
        match self.harness_offers(harness) {
            Some(offers) => offer_picks(offers, None, harness),
            None => settings_picks(self.settings.models(harness), None, harness),
        }
    }

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
                let harness = structured.harness;
                let fresh = chat.messages.is_empty() && structured.is_fresh();
                let others = if fresh {
                    CHAT_HARNESSES
                        .into_iter()
                        .filter(|other| *other != harness && self.chats_on(*other))
                        .flat_map(|other| self.harness_picks(other))
                        .collect()
                } else {
                    Vec::new()
                };
                let offered = Offered {
                    fallback: self.harness_offers(harness),
                    settings: if harness == AgentPreset::ClaudeCode {
                        claude_models
                    } else {
                        Vec::new()
                    },
                    others,
                };
                let account_usage = if harness == AgentPreset::Gateway {
                    AccountUsage::default()
                } else {
                    self.account_usage
                };
                let usage = structured
                    .usage
                    .map_or(account_usage, |usage| AccountUsage {
                        five_hour: usage.five_hour,
                        seven_day: usage.seven_day,
                    });
                structured.facts(offered, usage)
            }
        }
    }
}
