use gpui_kit::component::switch::Switch;
use gpui_kit::{div, px, AnyElement, Context, IntoElement, ParentElement, Styled};
use krad_chat::{ModelOffer, ModelSource};
use krad_domain::ModelId;

use super::super::gitlab::page::section_heading;
use super::super::{form_row, hint, panel_card, EnvironmentShell};
use crate::theme::{palette, Palette};

const MODELS_TITLE: &str = "Models in the picker";
const MODELS_HINT: &str =
    "Pick the Gateway models routed chats can use; the first one is the default.";
const MODELS_EMPTY: &str = "Connect to the Gateway to list its models.";
const NOT_SERVED: &str = "not served now";
const MODEL_SWITCH_PREFIX: &str = "gateway-model";
const FIELD_GAP: f32 = 10.0;

struct ModelRow {
    id: ModelId,
    owner: Option<String>,
    served: bool,
    picked: bool,
}

fn model_rows(served: &[ModelOffer], curated: &[ModelId]) -> Vec<ModelRow> {
    let mut rows: Vec<ModelRow> = curated
        .iter()
        .filter(|model| !served.iter().any(|offer| offer.value == model.as_str()))
        .map(|model| ModelRow {
            id: model.clone(),
            owner: None,
            served: false,
            picked: true,
        })
        .collect();
    rows.extend(served.iter().filter_map(|offer| {
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
    }));
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
        let curated = self.settings.gateway.models();
        let rows = model_rows(self.gateway_served(), &curated);
        let shown: Vec<AnyElement> = if rows.is_empty() {
            vec![hint(MODELS_EMPTY, pal)]
        } else {
            rows.into_iter()
                .enumerate()
                .map(|(index, row)| Self::gateway_model_row(index, &row, pal, cx))
                .collect()
        };
        div()
            .flex()
            .flex_col()
            .gap(px(FIELD_GAP))
            .child(section_heading(MODELS_TITLE, Some(MODELS_HINT), pal))
            .child(panel_card(shown, pal))
            .into_any_element()
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

    fn model(id: &str) -> ModelId {
        ModelId::parse(id).unwrap_or_else(|| unreachable!("{id} is a model id"))
    }

    fn offer(id: &str, owner: &str) -> ModelOffer {
        ModelOffer {
            value: id.to_owned(),
            name: id.to_owned(),
            description: None,
            efforts: Vec::new(),
            thinking_off: false,
            source: Some(ModelSource::Provider(owner.to_owned())),
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
