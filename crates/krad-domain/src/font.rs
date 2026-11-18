use serde::de::IgnoredAny;
use serde::{Deserialize, Deserializer, Serialize};

use crate::settings::Ligatures;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(as = "u8"))]
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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
enum ReadFont {
    Choice(FontChoice),
    Unreadable(IgnoredAny),
}

/// # Errors
///
/// Fails only when the deserializer itself fails; an unreadable value reads as the default font.
pub fn lenient_font<'de, D: Deserializer<'de>>(deserializer: D) -> Result<FontChoice, D::Error> {
    Ok(match ReadFont::deserialize(deserializer)? {
        ReadFont::Choice(choice) => choice,
        ReadFont::Unreadable(_) => FontChoice::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    struct Holder {
        #[serde(deserialize_with = "lenient_font")]
        font: FontChoice,
    }

    fn read(json: &str) -> Option<FontChoice> {
        serde_json::from_str::<Holder>(json)
            .ok()
            .map(|holder| holder.font)
    }

    #[test]
    fn an_unreadable_font_reads_as_the_default_and_a_choice_is_kept() {
        assert_eq!(
            read(r#"{"font":"JetBrainsMono"}"#),
            Some(FontChoice::default())
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
