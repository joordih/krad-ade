use krad_agents::AgentStatus;
use krad_chat::{ModelOffer, ModelSource};
use krad_domain::{AgentPreset, Effort, ModelId, Route, TerminalId};

use super::super::model_picker::HarnessRoute;
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
    pub(super) lane: HarnessRoute,
    pub(super) source: Option<ModelSource>,
}

pub(super) struct Facts {
    pub(super) route: Route,
    pub(super) lanes: Vec<HarnessRoute>,
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

const CLAUDE_GATEWAY: HarnessRoute = HarnessRoute {
    harness: AgentPreset::ClaudeCode,
    route: Route::Gateway,
};

fn settings_picks(
    models: Vec<ModelId>,
    current: Option<&str>,
    lane: HarnessRoute,
) -> Vec<ModelPick> {
    models
        .into_iter()
        .map(|model| ModelPick {
            current: current.is_some_and(|current| current.contains(model.as_str())),
            label: model.as_str().to_owned(),
            id: model,
            lane,
            source: None,
        })
        .collect()
}

pub(in crate::environment_shell) fn curated_offers(
    curated: Vec<ModelId>,
    served: &[ModelOffer],
) -> Vec<ModelOffer> {
    curated
        .into_iter()
        .map(|model| {
            served
                .iter()
                .find(|offer| offer.value == model.as_str())
                .cloned()
                .unwrap_or_else(|| ModelOffer {
                    value: model.as_str().to_owned(),
                    name: model.as_str().to_owned(),
                    description: None,
                    efforts: Effort::ALL.to_vec(),
                    thinking_off: false,
                    source: None,
                })
        })
        .collect()
}

fn offer_picks(offers: &[ModelOffer], current: Option<&str>, lane: HarnessRoute) -> Vec<ModelPick> {
    offers
        .iter()
        .filter_map(|offer| {
            Some(ModelPick {
                id: ModelId::parse(&offer.value)?,
                label: offer.name.clone(),
                current: current == Some(offer.value.as_str()),
                lane,
                source: offer.source.clone(),
            })
        })
        .collect()
}

fn transcript_facts(
    status: AgentStatus,
    models: Vec<ModelId>,
    account: AccountUsage,
    route: Route,
) -> Facts {
    let lane = HarnessRoute {
        harness: TRANSCRIPT_HARNESS,
        route,
    };
    let usage = match route {
        Route::Direct => AccountUsage {
            five_hour: status.five_hour_used.or(account.five_hour),
            seven_day: status.seven_day_used.or(account.seven_day),
        },
        Route::Gateway => AccountUsage::default(),
    };
    Facts {
        route,
        lanes: vec![lane],
        models: settings_picks(models, status.model_id.as_deref(), lane),
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
        usage,
    }
}

struct Offered<'a> {
    fallback: Option<&'a [ModelOffer]>,
    gateway: Vec<ModelOffer>,
    settings: Vec<ModelId>,
    others: Vec<ModelPick>,
    lanes: Vec<HarnessRoute>,
}

impl StructuredChat {
    fn lane(&self) -> HarnessRoute {
        HarnessRoute {
            harness: self.harness,
            route: self.route,
        }
    }

    fn facts(&self, offered: Offered<'_>, usage: AccountUsage) -> Facts {
        let lane = self.lane();
        let offers = match self.route {
            Route::Direct => self
                .catalog
                .as_ref()
                .map(|catalog| catalog.models.as_slice())
                .or(offered.fallback),
            Route::Gateway => Some(offered.gateway.as_slice()),
        };
        let chosen = self.chosen_model.as_ref();
        let shown = offers.and_then(|offers| {
            offers
                .iter()
                .find(|offer| chosen.is_some_and(|chosen| chosen.as_str() == offer.value))
                .or_else(|| offers.first())
        });
        let mut models = offers.map_or_else(
            || settings_picks(offered.settings, chosen.map(ModelId::as_str), lane),
            |offers| offer_picks(offers, shown.map(|shown| shown.value.as_str()), lane),
        );
        models.extend(offered.others);
        let all_efforts = match self.harness {
            AgentPreset::ClaudeCode => Effort::ALL.to_vec(),
            _ => Vec::new(),
        };
        let (usage, account) = match self.route {
            Route::Direct => (
                usage,
                self.catalog
                    .as_ref()
                    .and_then(|catalog| catalog.subscription.clone()),
            ),
            Route::Gateway => (AccountUsage::default(), None),
        };
        Facts {
            route: self.route,
            lanes: offered.lanes,
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
            account,
            usage,
        }
    }
}

const CHAT_HARNESSES: [AgentPreset; 3] = [
    AgentPreset::ClaudeCode,
    AgentPreset::Opencode,
    AgentPreset::Codex,
];

impl EnvironmentShell {
    fn harness_offers(&self, harness: AgentPreset) -> Option<&[ModelOffer]> {
        let offers = match harness {
            AgentPreset::Opencode => self.opencode_models(),
            other => self.harness_models.of(other),
        };
        (!offers.is_empty()).then_some(offers)
    }

    fn harness_picks(&self, harness: AgentPreset) -> Vec<ModelPick> {
        let lane = HarnessRoute::direct(harness);
        match self.harness_offers(harness) {
            Some(offers) => offer_picks(offers, None, lane),
            None => settings_picks(self.settings.models(harness), None, lane),
        }
    }

    fn gateway_choices(&self) -> Vec<ModelOffer> {
        curated_offers(self.settings.gateway.models(), self.gateway_offers())
    }

    fn other_lane_picks(&self, chat: &StructuredChat, fresh: bool) -> Vec<ModelPick> {
        let mut picks = Vec::new();
        if chat.harness == AgentPreset::ClaudeCode {
            match chat.route {
                Route::Direct if self.gateway_offered() => {
                    picks.extend(offer_picks(&self.gateway_choices(), None, CLAUDE_GATEWAY));
                }
                Route::Gateway => picks.extend(self.harness_picks(AgentPreset::ClaudeCode)),
                Route::Direct => {}
            }
        }
        if fresh {
            picks.extend(
                CHAT_HARNESSES
                    .into_iter()
                    .filter(|other| *other != chat.harness && self.native_agents.contains(other))
                    .flat_map(|other| self.harness_picks(other)),
            );
        }
        picks
    }

    fn chat_lanes(&self, chat: &StructuredChat) -> Vec<HarnessRoute> {
        let own = chat.lane();
        let gateway = chat.harness == AgentPreset::ClaudeCode && self.gateway_offered();
        let mut lanes = vec![own];
        if gateway {
            lanes.extend([
                HarnessRoute::direct(AgentPreset::ClaudeCode),
                CLAUDE_GATEWAY,
            ]);
            lanes.sort_by_key(|lane| lane.route != Route::Direct);
            lanes.dedup();
        }
        lanes
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
                let route = self
                    .terminal_route(&self.active, id)
                    .for_harness(TRANSCRIPT_HARNESS);
                let models = match route {
                    Route::Direct => claude_models,
                    Route::Gateway => self.settings.gateway.models(),
                };
                transcript_facts(status, models, self.account_usage, route)
            }
            Backend::Structured(structured) => {
                let harness = structured.harness;
                let fresh = chat.messages.is_empty() && structured.is_fresh();
                let offered = Offered {
                    fallback: self.harness_offers(harness),
                    gateway: self.gateway_choices(),
                    settings: if harness == AgentPreset::ClaudeCode {
                        claude_models
                    } else {
                        Vec::new()
                    },
                    others: self.other_lane_picks(structured, fresh),
                    lanes: self.chat_lanes(structured),
                };
                let usage = structured
                    .usage
                    .map_or(self.account_usage, |usage| AccountUsage {
                        five_hour: usage.five_hour,
                        seven_day: usage.seven_day,
                    });
                structured.facts(offered, usage)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(id: &str) -> ModelId {
        ModelId::parse(id).unwrap_or_else(|| unreachable!("{id} is a model id"))
    }

    #[test]
    fn curated_gateway_models_keep_their_order_even_when_no_longer_served() {
        let served = [ModelOffer {
            value: "gpt-5".to_owned(),
            name: "gpt-5".to_owned(),
            description: None,
            efforts: Vec::new(),
            thinking_off: false,
            source: Some(ModelSource::Provider("openai".to_owned())),
        }];
        let offers = curated_offers(vec![model("gone-model"), model("gpt-5")], &served);
        let ids: Vec<&str> = offers.iter().map(|offer| offer.value.as_str()).collect();
        assert_eq!(ids, ["gone-model", "gpt-5"]);
        assert_eq!(offers[1].source, served[0].source);
        assert_eq!(offers[0].efforts, Effort::ALL.to_vec());
    }
}
