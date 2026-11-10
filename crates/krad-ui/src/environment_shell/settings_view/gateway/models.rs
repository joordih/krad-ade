use gpui_kit::component::switch::Switch;
use gpui_kit::{AnyElement, Context, IntoElement};
use krad_chat::{GatewayModel, ModelSource};
use krad_domain::ModelId;

use super::super::sections::{empty_row, settings_section};
use super::super::{form_row, panel_card, EnvironmentShell};
use crate::theme::{palette, Palette};

const MODELS_TITLE: &str = "Models";
const MODELS_HINT: &str =
    "Turn on the models you want in the chat picker. The first one is the default.";
const MODELS_EMPTY: &str = "Its models show up here once the Gateway is connected.";
const NOT_SERVED: &str = "not served now";
const MODEL_SWITCH_PREFIX: &str = "gateway-model";

struct ModelRow {
    id: ModelId,
    owner: Option<String>,
    served: bool,
    picked: bool,
}

fn model_rows(served: &[GatewayModel], curated: &[ModelId]) -> Vec<ModelRow> {
    let mut rows: Vec<ModelRow> = curated
        .iter()
        .filter(|model| {
            !served
                .iter()
                .any(|served| served.offer.value == model.as_str())
        })
        .map(|model| ModelRow {
            id: model.clone(),
            owner: None,
            served: false,
            picked: true,
        })
        .collect();
    rows.extend(
        served
            .iter()
            .map(|served| &served.offer)
            .filter_map(|offer| {
                let id = ModelId::parse(&offer.value)?;
                Some(ModelRow {
                    picked: curated.contains(&id),
                    owner: match &offer.source {
                        Some(ModelSource::Provider(owner)) => Some(owner.clone()),
                        _ => None,
                    },
                    id,
                    served: true,
                })
            }),
    );
    rows
}

impl EnvironmentShell {
    fn toggle_gateway_model(&mut self, model: &ModelId, cx: &mut Context<Self>) {
        self.settings.gateway.toggle_model(model);
        self.refresh_gateway_offers();
        self.save_settings(cx);
        cx.notify();
    }

    pub(super) fn gateway_models_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let rows = model_rows(self.gateway_served(), &self.settings.gateway.models);
        let shown: Vec<AnyElement> = if rows.is_empty() {
            vec![empty_row(MODELS_EMPTY, pal)]
        } else {
            rows.into_iter()
                .enumerate()
                .map(|(index, row)| Self::gateway_model_row(index, &row, pal, cx))
                .collect()
        };
        settings_section(
            MODELS_TITLE,
            Some(MODELS_HINT),
            [panel_card(shown, pal)],
            pal,
        )
    }

    fn gateway_model_row(
        index: usize,
        row: &ModelRow,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let detail = match (row.served, &row.owner) {
            (false, _) => Some(NOT_SERVED.to_owned()),
            (true, owner) => owner.clone(),
        };
        let model = row.id.clone();
        let toggle = Switch::new((
            MODEL_SWITCH_PREFIX,
            u64::try_from(index).unwrap_or(u64::MAX),
        ))
        .checked(row.picked)
        .on_click(cx.listener(move |this: &mut Self, _: &bool, _, cx| {
            this.toggle_gateway_model(&model, cx);
        }))
        .into_any_element();
        form_row(row.id.as_str(), detail.as_deref(), toggle, pal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_chat::{ModelOffer, Vendor};

    fn model(id: &str) -> ModelId {
        ModelId::parse(id).unwrap_or_else(|| unreachable!("{id} is a model id"))
    }

    fn offer(id: &str, owner: &str) -> GatewayModel {
        GatewayModel {
            offer: ModelOffer {
                value: id.to_owned(),
                name: id.to_owned(),
                source: Some(ModelSource::Provider(owner.to_owned())),
                ..ModelOffer::default()
            },
            vendor: Vendor::from(owner),
        }
    }

    #[test]
    fn picked_models_the_gateway_no_longer_serves_come_first_and_are_flagged() {
        let served = [offer("gpt-5", "openai"), offer("gemini-3-pro", "google")];
        let rows = model_rows(&served, &[model("gone"), model("gemini-3-pro")]);
        let summary: Vec<(&str, bool, bool)> = rows
            .iter()
            .map(|row| (row.id.as_str(), row.served, row.picked))
            .collect();
        assert_eq!(
            summary,
            [
                ("gone", false, true),
                ("gpt-5", true, false),
                ("gemini-3-pro", true, true)
            ]
        );
        assert_eq!(rows[1].owner.as_deref(), Some("openai"));
    }
}
