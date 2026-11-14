use krad_domain::{GatewayModel, ModelId, ModelOffer};
use krad_protocol::GatewayState;

#[derive(Default)]
pub(in crate::environment_shell) struct GatewayMirror {
    pub(in crate::environment_shell) state: GatewayState,
    offers: Vec<ModelOffer>,
}

impl GatewayMirror {
    pub(in crate::environment_shell) fn served(&self) -> &[GatewayModel] {
        match &self.state {
            GatewayState::Ready { models, .. } => models,
            GatewayState::Unset | GatewayState::Checking | GatewayState::Failed { .. } => &[],
        }
    }

    pub(in crate::environment_shell) fn offers(&self) -> &[ModelOffer] {
        &self.offers
    }

    pub(in crate::environment_shell) const fn ready(&self) -> bool {
        matches!(self.state, GatewayState::Ready { .. })
    }

    pub(in crate::environment_shell) fn offer(&mut self, chosen: &[ModelId]) {
        let offers = chosen
            .iter()
            .filter_map(|model| {
                self.served()
                    .iter()
                    .find(|served| served.offer.value == model.as_str())
                    .map(|served| served.offer.clone())
            })
            .collect();
        self.offers = offers;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{LoginPlace, Vendor};

    fn served(value: &str) -> GatewayModel {
        GatewayModel {
            offer: ModelOffer {
                value: value.to_owned(),
                name: value.to_owned(),
                ..ModelOffer::default()
            },
            vendor: Vendor::Anthropic,
        }
    }

    #[test]
    fn only_the_chosen_models_the_gateway_serves_are_offered(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut gateway = GatewayMirror::default();
        let chosen = [
            ModelId::try_from("opus".to_owned())?,
            ModelId::try_from("gone".to_owned())?,
        ];
        gateway.offer(&chosen);
        assert!(gateway.offers().is_empty() && !gateway.ready());
        gateway.state = GatewayState::Ready {
            place: LoginPlace::Keyring,
            models: vec![served("opus"), served("haiku")],
        };
        gateway.offer(&chosen);
        let offered: Vec<&str> = gateway
            .offers()
            .iter()
            .map(|offer| offer.value.as_str())
            .collect();
        assert_eq!(offered, ["opus"]);
        assert!(gateway.ready());
        Ok(())
    }
}
