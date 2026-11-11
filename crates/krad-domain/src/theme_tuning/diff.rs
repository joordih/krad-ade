use crate::theme::{ThemeAppearance, ThemeColor};
use serde::{Deserialize, Serialize};

const COLOR_BLIND_LIGHT: DiffTriple = DiffTriple {
    added: ThemeColor(0x0000_5fcc),
    removed: ThemeColor(0x00c2_3b14),
    modified: ThemeColor(0x0086_5d00),
};
const COLOR_BLIND_DARK: DiffTriple = DiffTriple {
    added: ThemeColor(0x0052_a8ff),
    removed: ThemeColor(0x00ff_7a4d),
    modified: ThemeColor(0x00e6_b84d),
};
const HIGH_CONTRAST_LIGHT: DiffTriple = DiffTriple {
    added: ThemeColor(0x0000_6b1a),
    removed: ThemeColor(0x00b0_0014),
    modified: ThemeColor(0x0075_4c00),
};
const HIGH_CONTRAST_DARK: DiffTriple = DiffTriple {
    added: ThemeColor(0x0040_ff80),
    removed: ThemeColor(0x00ff_5266),
    modified: ThemeColor(0x00ff_d940),
};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct DiffTriple {
    pub added: ThemeColor,
    pub removed: ThemeColor,
    pub modified: ThemeColor,
}

impl DiffTriple {
    #[must_use]
    pub const fn get(&self, slot: DiffColorSlot) -> ThemeColor {
        match slot {
            DiffColorSlot::Added => self.added,
            DiffColorSlot::Removed => self.removed,
            DiffColorSlot::Modified => self.modified,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum DiffColorSlot {
    Added,
    Removed,
    Modified,
}

impl DiffColorSlot {
    pub const ALL: [Self; 3] = [Self::Added, Self::Removed, Self::Modified];

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Added => "Added",
            Self::Removed => "Removed",
            Self::Modified => "Modified",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffPreset {
    #[default]
    Default,
    ColorBlind,
    HighContrast,
}

impl DiffPreset {
    pub const ALL: [Self; 3] = [Self::Default, Self::ColorBlind, Self::HighContrast];

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::ColorBlind => "Red and blue",
            Self::HighContrast => "High contrast",
        }
    }

    #[must_use]
    pub const fn colors(self, appearance: ThemeAppearance) -> Option<DiffTriple> {
        match (self, appearance) {
            (Self::Default, _) => None,
            (Self::ColorBlind, ThemeAppearance::Light) => Some(COLOR_BLIND_LIGHT),
            (Self::ColorBlind, ThemeAppearance::Dark) => Some(COLOR_BLIND_DARK),
            (Self::HighContrast, ThemeAppearance::Light) => Some(HIGH_CONTRAST_LIGHT),
            (Self::HighContrast, ThemeAppearance::Dark) => Some(HIGH_CONTRAST_DARK),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub struct DiffColors {
    #[serde(default)]
    pub preset: DiffPreset,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added: Option<ThemeColor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removed: Option<ThemeColor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<ThemeColor>,
}

impl DiffColors {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    #[must_use]
    pub const fn custom(&self, slot: DiffColorSlot) -> Option<ThemeColor> {
        match slot {
            DiffColorSlot::Added => self.added,
            DiffColorSlot::Removed => self.removed,
            DiffColorSlot::Modified => self.modified,
        }
    }

    pub fn set_custom(&mut self, slot: DiffColorSlot, color: Option<ThemeColor>) {
        match slot {
            DiffColorSlot::Added => self.added = color,
            DiffColorSlot::Removed => self.removed = color,
            DiffColorSlot::Modified => self.modified = color,
        }
    }

    #[must_use]
    pub fn resolve(&self, appearance: ThemeAppearance, theme: DiffTriple) -> DiffTriple {
        let base = self.preset.colors(appearance).unwrap_or(theme);
        DiffTriple {
            added: self.added.unwrap_or(base.added),
            removed: self.removed.unwrap_or(base.removed),
            modified: self.modified.unwrap_or(base.modified),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const THEME: DiffTriple = DiffTriple {
        added: ThemeColor(0x0000_ff00),
        removed: ThemeColor(0x00ff_0000),
        modified: ThemeColor(0x00ff_ff00),
    };

    #[test]
    fn the_default_preset_keeps_the_theme_colours() {
        for appearance in [ThemeAppearance::Light, ThemeAppearance::Dark] {
            assert_eq!(DiffColors::default().resolve(appearance, THEME), THEME);
        }
    }

    #[test]
    fn presets_replace_every_colour_per_appearance() {
        for preset in [DiffPreset::ColorBlind, DiffPreset::HighContrast] {
            let colors = DiffColors {
                preset,
                ..DiffColors::default()
            };
            let light = colors.resolve(ThemeAppearance::Light, THEME);
            let dark = colors.resolve(ThemeAppearance::Dark, THEME);
            assert_ne!(light, dark, "{preset:?}");
            for slot in DiffColorSlot::ALL {
                assert_ne!(light.get(slot), THEME.get(slot), "{preset:?} {slot:?}");
            }
        }
    }

    #[test]
    fn the_red_and_blue_preset_adds_in_blue() {
        let added = DiffPreset::ColorBlind
            .colors(ThemeAppearance::Dark)
            .map(|colors| colors.added.channels());
        let Some([red, _, blue]) = added else {
            unreachable!("the preset has colours");
        };
        assert!(blue > red);
    }

    #[test]
    fn a_custom_colour_wins_over_its_preset_and_only_for_its_slot() {
        let mut colors = DiffColors {
            preset: DiffPreset::HighContrast,
            ..DiffColors::default()
        };
        colors.set_custom(DiffColorSlot::Removed, Some(ThemeColor(0x0012_3456)));
        let resolved = colors.resolve(ThemeAppearance::Light, THEME);
        assert_eq!(resolved.removed, ThemeColor(0x0012_3456));
        assert_eq!(resolved.added, HIGH_CONTRAST_LIGHT.added);
        assert_eq!(
            colors.custom(DiffColorSlot::Removed),
            Some(ThemeColor(0x0012_3456))
        );
        assert_eq!(colors.custom(DiffColorSlot::Added), None);
    }
}
