use serde::{Deserialize, Deserializer, Serialize};

use crate::settings::{Ligatures, TerminalFont};

const LEGACY_JETBRAINS_MONO: &str = "JetBrains Mono";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(from = "u8", into = "u8")]
pub struct FontSize(u8);

impl FontSize {
    pub const MIN: u8 = 8;
    pub const MAX: u8 = 32;

    #[must_use]
    pub const fn new(points: u8) -> Self {
        if points < Self::MIN {
            Self(Self::MIN)
        } else if points > Self::MAX {
            Self(Self::MAX)
        } else {
            Self(points)
        }
    }

    #[must_use]
    pub const fn points(self) -> u8 {
        self.0
    }

    #[must_use]
    pub fn as_f32(self) -> f32 {
        f32::from(self.0)
    }
}

impl From<u8> for FontSize {
    fn from(points: u8) -> Self {
        Self::new(points)
    }
}

impl From<FontSize> for u8 {
    fn from(size: FontSize) -> Self {
        size.0
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FontChoice {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<FontSize>,
    #[serde(default)]
    pub ligatures: Ligatures,
}

impl FontChoice {
    #[must_use]
    pub fn size_or(&self, default: FontSize) -> FontSize {
        self.size.unwrap_or(default)
    }

    #[must_use]
    pub fn family_or<'a>(&'a self, default: &'a str) -> &'a str {
        self.family.as_deref().unwrap_or(default)
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StoredFont {
    Legacy(TerminalFont),
    Choice(FontChoice),
}

impl From<StoredFont> for FontChoice {
    fn from(stored: StoredFont) -> Self {
        match stored {
            StoredFont::Choice(choice) => choice,
            StoredFont::Legacy(TerminalFont::System) => Self::default(),
            StoredFont::Legacy(TerminalFont::JetBrainsMono) => Self {
                family: Some(LEGACY_JETBRAINS_MONO.to_owned()),
                ..Self::default()
            },
        }
    }
}

/// # Errors
///
/// Fails when the value is neither an old terminal font name nor a font choice.
pub fn stored_font<'de, D: Deserializer<'de>>(deserializer: D) -> Result<FontChoice, D::Error> {
    StoredFont::deserialize(deserializer).map(FontChoice::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    struct Holder {
        #[serde(deserialize_with = "stored_font")]
        font: FontChoice,
    }

    fn read(json: &str) -> Option<FontChoice> {
        serde_json::from_str::<Holder>(json)
            .ok()
            .map(|holder| holder.font)
    }

    #[test]
    fn old_terminal_font_names_become_font_choices() {
        assert_eq!(read(r#"{"font":"System"}"#), Some(FontChoice::default()));
        assert_eq!(
            read(r#"{"font":"JetBrainsMono"}"#).and_then(|font| font.family),
            Some("JetBrains Mono".to_owned())
        );
        let full = read(r#"{"font":{"family":"Fira Code","size":40,"ligatures":"Off"}}"#);
        assert_eq!(
            full,
            Some(FontChoice {
                family: Some("Fira Code".to_owned()),
                size: Some(FontSize::new(FontSize::MAX)),
                ligatures: Ligatures::Off,
            })
        );
    }
}
