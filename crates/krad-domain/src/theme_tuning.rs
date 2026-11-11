mod diff;
mod scale;

use crate::hue::Hue;
use crate::ids::EnvironmentName;
use crate::settings::ThemeSelection;
use crate::theme::{ThemeAppearance, ThemeColor, ThemeId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub use diff::{DiffColorSlot, DiffColors, DiffPreset, DiffTriple};
pub use scale::InterfaceScale;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
#[serde(from = "u8", into = "u8")]
pub struct Percent(u8);

impl Percent {
    pub const MAX: u8 = 100;

    #[must_use]
    pub const fn new(percent: u8) -> Self {
        if percent > Self::MAX {
            Self(Self::MAX)
        } else {
            Self(percent)
        }
    }

    #[must_use]
    pub const fn value(self) -> u8 {
        self.0
    }

    #[must_use]
    pub fn fraction(self) -> f32 {
        f32::from(self.0) / f32::from(Self::MAX)
    }
}

impl From<u8> for Percent {
    fn from(percent: u8) -> Self {
        Self::new(percent)
    }
}

impl From<Percent> for u8 {
    fn from(percent: Percent) -> Self {
        percent.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
#[serde(from = "u16", into = "u16")]
pub struct HueDegrees(u16);

impl HueDegrees {
    pub const MAX: u16 = 360;

    #[must_use]
    pub const fn new(degrees: u16) -> Self {
        if degrees > Self::MAX {
            Self(Self::MAX)
        } else {
            Self(degrees)
        }
    }

    #[must_use]
    pub const fn value(self) -> u16 {
        self.0
    }
}

impl From<u16> for HueDegrees {
    fn from(degrees: u16) -> Self {
        Self::new(degrees)
    }
}

impl From<HueDegrees> for u16 {
    fn from(hue: HueDegrees) -> Self {
        hue.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(from = "u8", into = "u8")]
pub struct DarkLightness(Percent);

impl DarkLightness {
    pub const MIN: u8 = 0;
    pub const MAX: u8 = 30;
    pub const DEFAULT: u8 = 9;

    #[must_use]
    pub const fn new(percent: u8) -> Self {
        if percent > Self::MAX {
            Self(Percent::new(Self::MAX))
        } else {
            Self(Percent::new(percent))
        }
    }

    #[must_use]
    pub const fn value(self) -> u8 {
        self.0.value()
    }
}

impl Default for DarkLightness {
    fn default() -> Self {
        Self::new(Self::DEFAULT)
    }
}

impl From<u8> for DarkLightness {
    fn from(percent: u8) -> Self {
        Self::new(percent)
    }
}

impl From<DarkLightness> for u8 {
    fn from(lightness: DarkLightness) -> Self {
        lightness.value()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub struct SurfaceTone {
    #[serde(default)]
    pub hue: HueDegrees,
    #[serde(default)]
    pub saturation: Percent,
    #[serde(default)]
    pub dark_lightness: DarkLightness,
}

impl SurfaceTone {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccentChoice {
    #[default]
    Theme,
    Swatch(Hue),
    Custom(ThemeColor),
}

impl AccentChoice {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MainPaneMaterial {
    #[default]
    Glass,
    Opaque,
}

impl MainPaneMaterial {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    #[must_use]
    pub const fn is_glass(self) -> bool {
        matches!(self, Self::Glass)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub struct ThemeTuning {
    #[serde(default, skip_serializing_if = "AccentChoice::is_default")]
    pub accent: AccentChoice,
    #[serde(default, skip_serializing_if = "DiffColors::is_default")]
    pub diff: DiffColors,
    #[serde(default, skip_serializing_if = "SurfaceTone::is_default")]
    pub surfaces: SurfaceTone,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidebar_opacity: Option<Percent>,
    #[serde(default, skip_serializing_if = "MainPaneMaterial::is_default")]
    pub main_pane: MainPaneMaterial,
    #[serde(default, skip_serializing_if = "InterfaceScale::is_default")]
    pub interface_scale: InterfaceScale,
}

impl ThemeTuning {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EnvironmentTheme {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub light: Option<ThemeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dark: Option<ThemeId>,
}

impl EnvironmentTheme {
    #[must_use]
    pub const fn for_appearance(&self, appearance: ThemeAppearance) -> Option<&ThemeId> {
        match appearance {
            ThemeAppearance::Light => self.light.as_ref(),
            ThemeAppearance::Dark => self.dark.as_ref(),
        }
    }

    pub fn set(&mut self, appearance: ThemeAppearance, id: Option<ThemeId>) {
        match appearance {
            ThemeAppearance::Light => self.light = id,
            ThemeAppearance::Dark => self.dark = id,
        }
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.light.is_none() && self.dark.is_none()
    }

    #[must_use]
    pub fn over(&self, app: &ThemeSelection) -> ThemeSelection {
        ThemeSelection {
            light: self.light.clone().unwrap_or_else(|| app.light.clone()),
            dark: self.dark.clone().unwrap_or_else(|| app.dark.clone()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EnvironmentThemes(HashMap<EnvironmentName, EnvironmentTheme>);

impl EnvironmentThemes {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[must_use]
    pub fn of(&self, environment: &EnvironmentName) -> Option<&EnvironmentTheme> {
        self.0.get(environment)
    }

    pub fn set(
        &mut self,
        environment: &EnvironmentName,
        appearance: ThemeAppearance,
        id: Option<ThemeId>,
    ) {
        let theme = self.0.entry(environment.clone()).or_default();
        theme.set(appearance, id);
        if theme.is_empty() {
            self.0.remove(environment);
        }
    }

    #[must_use]
    pub fn selection(
        &self,
        environment: Option<&EnvironmentName>,
        app: &ThemeSelection,
    ) -> ThemeSelection {
        environment
            .and_then(|environment| self.of(environment))
            .map_or_else(|| app.clone(), |theme| theme.over(app))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(text: &str) -> EnvironmentName {
        EnvironmentName::new(text.to_owned())
    }

    #[test]
    fn percentages_hues_and_dark_lightness_are_capped() {
        assert_eq!(Percent::new(140).value(), Percent::MAX);
        assert_eq!(HueDegrees::new(400).value(), HueDegrees::MAX);
        assert_eq!(DarkLightness::new(90).value(), DarkLightness::MAX);
        assert_eq!(DarkLightness::default().value(), DarkLightness::DEFAULT);
    }

    #[test]
    fn an_environment_theme_falls_back_to_the_app_theme_per_appearance() {
        let app = ThemeSelection::default();
        let mut themes = EnvironmentThemes::default();
        let work = name("work");
        assert_eq!(themes.selection(Some(&work), &app), app);
        themes.set(
            &work,
            ThemeAppearance::Dark,
            Some(ThemeId("nord".to_owned())),
        );
        let picked = themes.selection(Some(&work), &app);
        assert_eq!(picked.dark, ThemeId("nord".to_owned()));
        assert_eq!(picked.light, app.light);
        assert_eq!(themes.selection(Some(&name("home")), &app), app);
        assert_eq!(themes.selection(None, &app), app);
        themes.set(&work, ThemeAppearance::Dark, None);
        assert!(themes.is_empty(), "an emptied environment is forgotten");
    }

    #[test]
    fn tuning_round_trips_and_stays_out_of_the_file_until_changed() -> serde_json::Result<()> {
        assert_eq!(serde_json::to_string(&ThemeTuning::default())?, "{}");
        let tuning = ThemeTuning {
            accent: AccentChoice::Custom(ThemeColor(0x00ff_8800)),
            diff: DiffColors {
                preset: DiffPreset::ColorBlind,
                added: Some(ThemeColor(0x0000_66cc)),
                ..DiffColors::default()
            },
            surfaces: SurfaceTone {
                hue: HueDegrees::new(210),
                saturation: Percent::new(12),
                dark_lightness: DarkLightness::new(14),
            },
            sidebar_opacity: Some(Percent::new(40)),
            main_pane: MainPaneMaterial::Opaque,
            interface_scale: InterfaceScale::new(120),
        };
        let back: ThemeTuning = serde_json::from_str(&serde_json::to_string(&tuning)?)?;
        assert_eq!(back, tuning);
        let swatch: ThemeTuning = serde_json::from_str(r#"{"accent":{"swatch":"green"}}"#)?;
        assert_eq!(swatch.accent, AccentChoice::Swatch(Hue::Green));
        Ok(())
    }

    #[test]
    fn environment_themes_round_trip_by_environment_name() -> serde_json::Result<()> {
        let mut themes = EnvironmentThemes::default();
        themes.set(
            &name("work"),
            ThemeAppearance::Light,
            Some(ThemeId("krad-paper".to_owned())),
        );
        let written = serde_json::to_string(&themes)?;
        assert_eq!(written, r#"{"work":{"light":"krad-paper"}}"#);
        let back: EnvironmentThemes = serde_json::from_str(&written)?;
        assert_eq!(back, themes);
        Ok(())
    }
}
