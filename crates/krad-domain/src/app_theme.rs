use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

const HEX_PREFIX: char = '#';
const HEX_DIGITS: usize = 6;
const HEX_RADIX: u32 = 16;
const RGBA_PREFIX: &str = "rgba(";
const RGBA_SUFFIX: char = ')';
const RGBA_SEPARATOR: char = ',';
const RED_SHIFT: u32 = 16;
const GREEN_SHIFT: u32 = 8;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AppThemeId(pub String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ColorParseError {
    Hex(String),
    Rgba(String),
}

impl fmt::Display for ColorParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hex(value) => write!(f, "expected a #rrggbb colour, found `{value}`"),
            Self::Rgba(value) => write!(f, "expected an rgba(r, g, b, a) colour, found `{value}`"),
        }
    }
}

impl std::error::Error for ColorParseError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub struct HexColor(pub u32);

impl FromStr for HexColor {
    type Err = ColorParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let invalid = || ColorParseError::Hex(value.to_owned());
        let digits = value.strip_prefix(HEX_PREFIX).ok_or_else(invalid)?;
        if digits.len() != HEX_DIGITS {
            return Err(invalid());
        }
        u32::from_str_radix(digits, HEX_RADIX)
            .map(Self)
            .map_err(|_| invalid())
    }
}

impl TryFrom<String> for HexColor {
    type Error = ColorParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(try_from = "String")]
pub struct Wash {
    pub color: u32,
    pub alpha: f32,
}

impl FromStr for Wash {
    type Err = ColorParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let invalid = || ColorParseError::Rgba(value.to_owned());
        let inner = value
            .strip_prefix(RGBA_PREFIX)
            .and_then(|rest| rest.strip_suffix(RGBA_SUFFIX))
            .ok_or_else(invalid)?;
        let parts: Vec<&str> = inner.split(RGBA_SEPARATOR).map(str::trim).collect();
        let [red, green, blue, alpha] = parts[..] else {
            return Err(invalid());
        };
        let channel = |part: &str| part.parse::<u8>().map(u32::from).map_err(|_| invalid());
        let alpha: f32 = alpha.parse().map_err(|_| invalid())?;
        if !(0.0..=1.0).contains(&alpha) {
            return Err(invalid());
        }
        Ok(Self {
            color: (channel(red)? << RED_SHIFT) | (channel(green)? << GREEN_SHIFT) | channel(blue)?,
            alpha,
        })
    }
}

impl TryFrom<String> for Wash {
    type Error = ColorParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ThemeTokens {
    pub backdrop_from: HexColor,
    pub backdrop_to: HexColor,
    pub text_strong: HexColor,
    pub text_default: HexColor,
    pub text_subtle: HexColor,
    pub text_muted: HexColor,
    pub text_disabled: HexColor,
    pub bg_surface: HexColor,
    pub bg_row_selected: HexColor,
    pub bg_panel: HexColor,
    pub bg_hover: Wash,
    pub bg_selected: Wash,
    pub border: Wash,
    pub divider: Wash,
    pub attention: HexColor,
    pub attention_text: HexColor,
    pub chip_bg: HexColor,
    pub chip_fg: HexColor,
    pub accent: HexColor,
    pub accent_hover: HexColor,
    pub accent_pressed: HexColor,
    pub on_accent: HexColor,
    pub success_fg: HexColor,
    pub warning_fg: HexColor,
    pub error_fg: HexColor,
    pub info_fg: HexColor,
    pub syntax_keyword: HexColor,
    pub syntax_string: HexColor,
    pub syntax_comment: HexColor,
    pub syntax_number: HexColor,
    pub syntax_type: HexColor,
    pub ansi_0: HexColor,
    pub ansi_1: HexColor,
    pub ansi_2: HexColor,
    pub ansi_3: HexColor,
    pub ansi_4: HexColor,
    pub ansi_5: HexColor,
    pub ansi_6: HexColor,
    pub ansi_7: HexColor,
    pub ansi_8: HexColor,
    pub ansi_9: HexColor,
    pub ansi_10: HexColor,
    pub ansi_11: HexColor,
    pub ansi_12: HexColor,
    pub ansi_13: HexColor,
    pub ansi_14: HexColor,
    pub ansi_15: HexColor,
}

impl ThemeTokens {
    #[must_use]
    pub const fn ansi(&self) -> [u32; 16] {
        [
            self.ansi_0.0,
            self.ansi_1.0,
            self.ansi_2.0,
            self.ansi_3.0,
            self.ansi_4.0,
            self.ansi_5.0,
            self.ansi_6.0,
            self.ansi_7.0,
            self.ansi_8.0,
            self.ansi_9.0,
            self.ansi_10.0,
            self.ansi_11.0,
            self.ansi_12.0,
            self.ansi_13.0,
            self.ansi_14.0,
            self.ansi_15.0,
        ]
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct AppTheme {
    pub id: AppThemeId,
    pub name: String,
    pub light: ThemeTokens,
    pub dark: ThemeTokens,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct ThemeCatalog {
    pub default: AppThemeId,
    pub themes: Vec<AppTheme>,
}

impl FromStr for ThemeCatalog {
    type Err = serde_json::Error;

    fn from_str(json: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(json)
    }
}

impl ThemeCatalog {
    #[must_use]
    pub fn resolve(&self, chosen: Option<&AppThemeId>) -> Option<usize> {
        let position = |id: &AppThemeId| self.themes.iter().position(|theme| theme.id == *id);
        chosen
            .and_then(position)
            .or_else(|| position(&self.default))
            .or_else(|| (!self.themes.is_empty()).then_some(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_colors_need_six_digits_after_a_hash() {
        assert_eq!("#007aff".parse(), Ok(HexColor(0x0000_7aff)));
        assert!("007aff".parse::<HexColor>().is_err());
        assert!("#fff".parse::<HexColor>().is_err());
        assert!("#gggggg".parse::<HexColor>().is_err());
    }

    #[test]
    fn washes_read_rgb_channels_and_alpha() {
        assert_eq!(
            "rgba(29, 29, 31, 0.05)".parse(),
            Ok(Wash {
                color: 0x001d_1d1f,
                alpha: 0.05
            })
        );
        assert!("rgba(256, 0, 0, 0.5)".parse::<Wash>().is_err());
        assert!("rgba(0, 0, 0, 1.5)".parse::<Wash>().is_err());
        assert!("rgb(0, 0, 0)".parse::<Wash>().is_err());
    }
}
