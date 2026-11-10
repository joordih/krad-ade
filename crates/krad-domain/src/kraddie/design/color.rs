use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

const HEX_MARK: char = '#';
const HEX_DIGITS: usize = 6;
const HEX_RADIX: u32 = 16;
const CHANNEL_BITS: u32 = 8;
const CHANNEL_MASK: u32 = 0xff;
const RGB_MASK: u32 = 0x00ff_ffff;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KraddieColorError;

impl fmt::Display for KraddieColorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("A color is written as #rrggbb")
    }
}

impl std::error::Error for KraddieColorError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct KraddieColor(u32);

impl KraddieColor {
    #[must_use]
    pub const fn rgb(rgb: u32) -> Self {
        Self(rgb & RGB_MASK)
    }

    #[must_use]
    pub const fn hex(self) -> u32 {
        self.0
    }

    #[must_use]
    pub fn channels(self) -> [u8; 3] {
        let channel =
            |shift: u32| u8::try_from((self.0 >> shift) & CHANNEL_MASK).unwrap_or_default();
        [channel(2 * CHANNEL_BITS), channel(CHANNEL_BITS), channel(0)]
    }
}

impl FromStr for KraddieColor {
    type Err = KraddieColorError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let digits = text
            .trim()
            .strip_prefix(HEX_MARK)
            .ok_or(KraddieColorError)?;
        if digits.len() != HEX_DIGITS {
            return Err(KraddieColorError);
        }
        u32::from_str_radix(digits, HEX_RADIX)
            .map(Self::rgb)
            .map_err(|_| KraddieColorError)
    }
}

impl TryFrom<String> for KraddieColor {
    type Error = KraddieColorError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<KraddieColor> for String {
    fn from(color: KraddieColor) -> Self {
        color.to_string()
    }
}

impl fmt::Display for KraddieColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{HEX_MARK}{:06x}", self.0)
    }
}
