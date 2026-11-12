use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComposerDesign {
    #[default]
    Classic,
    Tray,
}

impl ComposerDesign {
    pub const ALL: [Self; 2] = [Self::Classic, Self::Tray];

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Classic => "Current",
            Self::Tray => "New",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelPickerLayout {
    #[default]
    Side,
    Stacked,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BetaSettings {
    #[serde(default)]
    pub composer: ComposerDesign,
    #[serde(default)]
    pub model_picker: ModelPickerLayout,
}

impl BetaSettings {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_new_composer_round_trips_and_older_settings_keep_the_current_one(
    ) -> serde_json::Result<()> {
        let beta = BetaSettings {
            composer: ComposerDesign::Tray,
            model_picker: ModelPickerLayout::Stacked,
        };
        let text = serde_json::to_string(&beta)?;
        assert_eq!(text, r#"{"composer":"tray","model_picker":"stacked"}"#);
        assert_eq!(serde_json::from_str::<BetaSettings>(&text)?, beta);
        assert!(serde_json::from_str::<BetaSettings>("{}")?.is_default());
        Ok(())
    }
}
