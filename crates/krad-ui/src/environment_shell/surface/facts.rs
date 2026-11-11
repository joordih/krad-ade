use krad_agents::AgentStatus;
use krad_chat::{ModelOffer, ModelSource};
use krad_domain::{
    claude_alias_name, humanize_model_id, AgentPreset, ContextSize, Effort, EffortSnap,
    EnvironmentName, ModelId, TerminalId,
};
use std::path::Path;

use super::super::model_picker::{ModelChoice, ModelMarks};
use super::super::EnvironmentShell;
use super::structured::StructuredChat;
use super::transcript::TRANSCRIPT_HARNESS;
use super::{Backend, SurfaceChat};

pub(super) struct ModelPick {
    pub(super) id: ModelId,
    pub(super) label: String,
    pub(super) current: bool,
    pub(super) harness: AgentPreset,
    pub(super) source: Option<ModelSource>,
    pub(super) description: Option<String>,
    pub(super) recommended: bool,
    pub(super) context: Option<ContextSize>,
    pub(super) missing: bool,
}

pub(super) struct Facts {
    pub(super) model: Option<String>,
    pub(super) model_missing: bool,
    pub(super) models: Vec<ModelPick>,
    pub(super) effort: Option<Effort>,
    pub(super) efforts: Vec<Effort>,
    pub(super) default_effort: Option<Effort>,
    pub(super) offers_effort: bool,
    pub(super) effort_notice: Option<EffortSnap>,
    pub(super) fast: Option<bool>,
    pub(super) thinking: Option<bool>,
    pub(super) context: Option<u16>,
    pub(super) reports_context: bool,
    pub(super) context_window: Option<u64>,
    pub(super) account: Option<String>,
}

pub(in crate::environment_shell) fn model_label(id: &str) -> String {
    claude_alias_name(id)
        .map(str::to_owned)
        .or_else(|| humanize_model_id(id))
        .unwrap_or_else(|| id.to_owned())
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
            label: model_label(model.as_str()),
            context: krad_domain::split_context(model.as_str()).1,
            id: model,
            harness,
            source: None,
            description: None,
            recommended: false,
            missing: false,
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
                current: current.is_some_and(|current| offer.answers_to(current)),
                harness,
                source: offer.source.clone(),
                description: offer.description.clone(),
                recommended: offer.recommended,
                context: offer.context,
                missing: false,
            })
        })
        .collect()
}

fn missing_pick(model: &ModelId, harness: AgentPreset) -> ModelPick {
    ModelPick {
        id: model.clone(),
        label: model_label(model.as_str()),
        current: true,
        harness,
        source: None,
        description: None,
        recommended: false,
        context: krad_domain::split_context(model.as_str()).1,
        missing: true,
    }
}

fn transcript_facts(status: &AgentStatus, models: Vec<ModelId>) -> Facts {
    Facts {
        models: settings_picks(models, status.model_id.as_deref(), TRANSCRIPT_HARNESS),
        model: status.model_name.clone(),
        model_missing: false,
        default_effort: None,
        offers_effort: true,
        effort_notice: None,
        fast: None,
        effort: status
            .effort
            .as_deref()
            .and_then(|level| level.parse().ok()),
        efforts: TRANSCRIPT_HARNESS.offered_efforts().to_vec(),
        thinking: status.thinking,
        reports_context: status.context_used.is_some(),
        context: status.context_used,
        context_window: None,
        account: None,
    }
}

impl From<&ModelPick> for ModelChoice {
    fn from(pick: &ModelPick) -> Self {
        Self {
            id: Some(pick.id.clone()),
            label: pick.label.clone(),
            current: pick.current,
            section: pick.source.clone(),
            description: pick.description.clone(),
            recommended: pick.recommended,
            context: pick.context,
            missing: pick.missing,
            marks: ModelMarks::default(),
        }
    }
}

struct Offered<'a> {
    fallback: Option<&'a [ModelOffer]>,
    settings: Vec<ModelId>,
    others: Vec<ModelPick>,
}

impl StructuredChat {
    fn facts(&self, offered: Offered<'_>) -> Facts {
        let live = self.acp.live_models();
        let offers = live
            .as_ref()
            .map(|(offers, _)| offers.as_slice())
            .or_else(|| {
                self.catalog
                    .as_ref()
                    .map(|catalog| catalog.models.as_slice())
            })
            .or(offered.fallback);
        let chosen = live
            .as_ref()
            .and_then(|(_, current)| current.as_ref())
            .or(self.chosen_model.as_ref());
        let shown = offers.and_then(|offers| match chosen {
            Some(chosen) => offers
                .iter()
                .find(|offer| offer.answers_to(chosen.as_str())),
            None => offers
                .iter()
                .find(|offer| offer.recommended)
                .or_else(|| offers.first()),
        });
        let missing = chosen.filter(|_| offers.is_some() && shown.is_none());
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
        if let Some(missing) = missing {
            models.insert(0, missing_pick(missing, self.harness));
        }
        models.extend(offered.others);
        let offers_effort = !self.harness.offered_efforts().is_empty()
            || offers.is_some_and(|offers| offers.iter().any(|offer| !offer.efforts.is_empty()));
        let facts = Facts {
            model: shown
                .map(|offer| offer.name.clone())
                .or_else(|| missing.map(|model| model_label(model.as_str())))
                .or_else(|| self.model.clone()),
            model_missing: missing.is_some(),
            models,
            effort: self.effort,
            efforts: shown.map_or_else(
                || self.harness.offered_efforts().to_vec(),
                |offer| offer.efforts.clone(),
            ),
            default_effort: shown.and_then(|offer| offer.default_effort),
            offers_effort,
            effort_notice: self.effort_notice,
            thinking: shown
                .filter(|offer| offer.thinking_off)
                .map(|_| self.thinking),
            context: self.context_used(),
            reports_context: self.reports_context(),
            context_window: self.context_window,
            fast: None,
            account: self
                .catalog
                .as_ref()
                .and_then(|catalog| catalog.subscription.clone()),
        };
        match self.acp.effort() {
            Some(acp) => Facts {
                effort: acp.current,
                efforts: acp.levels,
                default_effort: None,
                offers_effort: true,
                effort_notice: None,
                fast: acp.fast,
                ..facts
            },
            None => facts,
        }
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn chats_on(
        &self,
        env: &EnvironmentName,
        root: Option<&Path>,
        harness: AgentPreset,
    ) -> bool {
        match harness {
            AgentPreset::Gateway => self.gateway_ready(),
            AgentPreset::ClaudeCode
            | AgentPreset::Codex
            | AgentPreset::Opencode
            | AgentPreset::Acp(_) => self.agents_for(env, root).contains(&harness),
            AgentPreset::Shell => false,
        }
    }

    pub(super) fn harness_offers(&self, harness: AgentPreset) -> Option<&[ModelOffer]> {
        let host = self.active_link().host();
        let offers = match harness {
            AgentPreset::Opencode => self.opencode_models_on(&host),
            AgentPreset::Gateway => self.gateway_offers(),
            AgentPreset::ClaudeCode | AgentPreset::Codex | AgentPreset::Acp(_) => {
                self.listed_models.of(&host, harness)
            }
            AgentPreset::Shell => &[],
        };
        (!offers.is_empty()).then_some(offers)
    }

    fn settings_models(&self, harness: AgentPreset) -> Vec<ModelId> {
        if harness.takes_settings_models() {
            self.settings.models(harness)
        } else {
            Vec::new()
        }
    }

    pub(in crate::environment_shell) fn model_choices(
        &self,
        harness: AgentPreset,
        current: Option<&ModelId>,
    ) -> Vec<ModelChoice> {
        self.harness_picks(harness, current)
            .iter()
            .map(ModelChoice::from)
            .collect()
    }

    fn harness_picks(&self, harness: AgentPreset, current: Option<&ModelId>) -> Vec<ModelPick> {
        let current = current.map(ModelId::as_str);
        match self.harness_offers(harness) {
            Some(offers) => offer_picks(offers, current, harness),
            None => settings_picks(self.settings_models(harness), current, harness),
        }
    }

    pub(super) fn composer_facts(&self, id: TerminalId, chat: &SurfaceChat) -> Facts {
        match &chat.backend {
            Backend::Transcript(_) => {
                let status = self
                    .envs
                    .get(&self.active)
                    .and_then(|live| live.statuses.get(&id))
                    .cloned()
                    .unwrap_or_default();
                transcript_facts(&status, self.settings_models(TRANSCRIPT_HARNESS))
            }
            Backend::Structured(structured) => {
                let harness = structured.harness;
                let fresh = chat.messages.is_empty() && structured.is_fresh();
                let root = self.active_group().map(|group| group.root.as_path());
                let others = if fresh {
                    let available = self.agents_for(&self.active, root);
                    AgentPreset::chat_harnesses(&self.settings.acp.all_agents())
                        .into_iter()
                        .filter(|other| {
                            other.native_twin() != harness.native_twin()
                                && other.is_offered(available)
                                && self.chats_on(&self.active, root, *other)
                        })
                        .flat_map(|other| self.harness_picks(other, None))
                        .collect()
                } else {
                    Vec::new()
                };
                let offered = Offered {
                    fallback: self.harness_offers(harness),
                    settings: self.settings_models(harness),
                    others,
                };
                structured.facts(offered)
            }
        }
    }
}
