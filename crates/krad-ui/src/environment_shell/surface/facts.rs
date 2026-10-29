use krad_agents::AgentStatus;
use krad_domain::{Effort, ModelId, TerminalId};

use super::super::EnvironmentShell;
use super::structured::StructuredChat;
use super::{Backend, SurfaceChat, SURFACE_HARNESS};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::environment_shell) struct AccountUsage {
    pub(in crate::environment_shell) five_hour: Option<u16>,
    pub(in crate::environment_shell) seven_day: Option<u16>,
}

pub(super) struct ModelPick {
    pub(super) id: ModelId,
    pub(super) label: String,
    pub(super) current: bool,
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

fn settings_picks(models: Vec<ModelId>, current: Option<&str>) -> Vec<ModelPick> {
    models
        .into_iter()
        .map(|model| ModelPick {
            current: current.is_some_and(|current| current.contains(model.as_str())),
            label: model.as_str().to_owned(),
            id: model,
        })
        .collect()
}

fn transcript_facts(status: AgentStatus, models: Vec<ModelId>, account: AccountUsage) -> Facts {
    Facts {
        models: settings_picks(models, status.model_id.as_deref()),
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

impl StructuredChat {
    fn facts(&self, models: Vec<ModelId>, usage: AccountUsage) -> Facts {
        let offers = self.catalog.as_ref().map(|catalog| &catalog.models);
        let chosen = self.chosen_model.as_ref();
        let shown = offers.and_then(|offers| {
            offers
                .iter()
                .find(|offer| chosen.is_some_and(|chosen| chosen.as_str() == offer.value))
                .or_else(|| offers.first())
        });
        let models = offers.map_or_else(
            || settings_picks(models, chosen.map(ModelId::as_str)),
            |offers| {
                offers
                    .iter()
                    .filter_map(|offer| {
                        Some(ModelPick {
                            id: ModelId::parse(&offer.value)?,
                            label: offer.name.clone(),
                            current: shown.is_some_and(|shown| shown.value == offer.value),
                        })
                    })
                    .collect()
            },
        );
        Facts {
            model: shown
                .map(|offer| offer.name.clone())
                .or_else(|| self.model.clone()),
            models,
            effort: self.effort,
            efforts: shown.map_or_else(|| Effort::ALL.to_vec(), |offer| offer.efforts.clone()),
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
        let models = self.settings.models(SURFACE_HARNESS);
        match &chat.backend {
            Backend::Transcript(_) => {
                let status = self
                    .envs
                    .get(&self.active)
                    .and_then(|live| live.statuses.get(&id))
                    .cloned()
                    .unwrap_or_default();
                transcript_facts(status, models, self.account_usage)
            }
            Backend::Structured(structured) => structured.facts(models, self.account_usage),
        }
    }
}
