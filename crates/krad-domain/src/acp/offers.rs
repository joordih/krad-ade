use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

macro_rules! agent_reported_id {
    ($($name:ident),+ $(,)?) => {
        $(
            #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
            #[serde(transparent)]
            pub struct $name(pub String);
        )+
    };
}

agent_reported_id!(
    AcpModeId,
    AcpModelId,
    AcpPermissionOptionId,
    AcpAuthMethodId,
    AcpConfigOptionId,
    AcpConfigValue
);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcpMode {
    pub id: AcpModeId,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcpModes {
    pub current: AcpModeId,
    pub available: Vec<AcpMode>,
}

impl AcpModes {
    #[must_use]
    pub fn current_mode(&self) -> Option<&AcpMode> {
        self.available.iter().find(|mode| mode.id == self.current)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcpModel {
    pub id: AcpModelId,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcpModels {
    pub current: AcpModelId,
    pub available: Vec<AcpModel>,
}

impl AcpModels {
    #[must_use]
    pub fn current_model(&self) -> Option<&AcpModel> {
        self.available.iter().find(|model| model.id == self.current)
    }

    #[must_use]
    pub fn offers(&self, id: &AcpModelId) -> bool {
        self.available.iter().any(|model| &model.id == id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcpConfigChoice {
    pub value: AcpConfigValue,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcpConfigOption {
    pub id: AcpConfigOptionId,
    pub name: String,
    pub description: Option<String>,
    pub current: AcpConfigValue,
    pub choices: Vec<AcpConfigChoice>,
}

impl AcpConfigOption {
    #[must_use]
    pub fn current_choice(&self) -> Option<&AcpConfigChoice> {
        self.choices
            .iter()
            .find(|choice| choice.value == self.current)
    }

    #[must_use]
    pub fn offers(&self, value: &AcpConfigValue) -> bool {
        self.choices.iter().any(|choice| &choice.value == value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcpPermissionKind {
    AllowOnce,
    AllowAlways,
    RejectOnce,
    RejectAlways,
}

impl AcpPermissionKind {
    #[must_use]
    pub const fn allows(self) -> bool {
        matches!(self, Self::AllowOnce | Self::AllowAlways)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcpPermissionOption {
    pub id: AcpPermissionOptionId,
    pub name: String,
    pub kind: AcpPermissionKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AcpSignIn {
    ByAgent,
    InTerminal {
        args: Vec<String>,
        env: BTreeMap<String, String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcpAuthMethod {
    pub id: AcpAuthMethodId,
    pub name: String,
    pub description: Option<String>,
    pub sign_in: AcpSignIn,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(id: &str) -> AcpModel {
        AcpModel {
            id: AcpModelId(id.to_owned()),
            name: id.to_uppercase(),
            description: None,
        }
    }

    #[test]
    fn the_current_model_is_the_offered_one_with_its_id() {
        let models = AcpModels {
            current: AcpModelId("b".to_owned()),
            available: vec![model("a"), model("b")],
        };
        assert_eq!(
            models.current_model().map(|model| model.name.as_str()),
            Some("B")
        );
        assert!(models.offers(&AcpModelId("a".to_owned())));
        assert!(!models.offers(&AcpModelId("c".to_owned())));
    }

    #[test]
    fn a_config_option_shows_its_current_choice_and_offers_only_its_own() {
        let choice = |value: &str| AcpConfigChoice {
            value: AcpConfigValue(value.to_owned()),
            name: value.to_uppercase(),
            description: None,
        };
        let option = AcpConfigOption {
            id: AcpConfigOptionId("effort".to_owned()),
            name: "Effort".to_owned(),
            description: None,
            current: AcpConfigValue("high".to_owned()),
            choices: vec![choice("low"), choice("high")],
        };
        assert_eq!(
            option.current_choice().map(|choice| choice.name.as_str()),
            Some("HIGH")
        );
        assert!(option.offers(&AcpConfigValue("low".to_owned())));
        assert!(!option.offers(&AcpConfigValue("max".to_owned())));
    }

    #[test]
    fn only_allow_options_allow() {
        assert!(AcpPermissionKind::AllowAlways.allows());
        assert!(!AcpPermissionKind::RejectOnce.allows());
    }
}
