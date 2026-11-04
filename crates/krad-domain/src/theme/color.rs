use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

const HEX_PREFIX: char = '#';
const HEX_DIGITS: usize = 6;
const HEX_RADIX: u32 = 16;
const CHANNEL_MAX: f32 = 255.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ThemeColor(pub u32);

#[derive(Debug, PartialEq, Eq)]
pub struct ThemeColorError(String);

impl fmt::Display for ThemeColorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "`{}` is not a #rrggbb color", self.0)
    }
}

impl std::error::Error for ThemeColorError {}

impl ThemeColor {
    #[must_use]
    pub const fn channels(self) -> [u8; 3] {
        let [_, red, green, blue] = self.0.to_be_bytes();
        [red, green, blue]
    }

    #[must_use]
    pub fn mix(self, other: Self, amount: f32) -> Self {
        let amount = amount.clamp(0.0, 1.0);
        let [ours, theirs] = [self.channels(), other.channels()];
        let blend = |index: usize| {
            let value = f32::from(ours[index])
                + (f32::from(theirs[index]) - f32::from(ours[index])) * amount;
            let clamped = value.round().clamp(0.0, CHANNEL_MAX);
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "the value is rounded and clamped to a channel's range"
            )]
            let channel = clamped as u8;
            u32::from(channel)
        };
        Self((blend(0) << 16) | (blend(1) << 8) | blend(2))
    }
}

impl FromStr for ThemeColor {
    type Err = ThemeColorError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let digits = text
            .strip_prefix(HEX_PREFIX)
            .filter(|digits| digits.len() == HEX_DIGITS)
            .ok_or_else(|| ThemeColorError(text.to_owned()))?;
        u32::from_str_radix(digits, HEX_RADIX)
            .map(Self)
            .map_err(|_| ThemeColorError(text.to_owned()))
    }
}

impl fmt::Display for ThemeColor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{HEX_PREFIX}{:06x}", self.0)
    }
}

impl Serialize for ThemeColor {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ThemeColor {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_read_and_write_as_hex_and_mix_channel_by_channel() {
        let red: ThemeColor = "#ff0000".parse().unwrap_or(ThemeColor(0));
        assert_eq!(red, ThemeColor(0x00ff_0000));
        assert_eq!(red.to_string(), "#ff0000");
        assert_eq!(red.mix(ThemeColor(0), 0.5), ThemeColor(0x0080_0000));
        assert!("ff0000".parse::<ThemeColor>().is_err());
        assert!("#ff00".parse::<ThemeColor>().is_err());
    }
}
