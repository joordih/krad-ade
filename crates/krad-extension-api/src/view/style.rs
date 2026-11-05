use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const MAX_FIXED_SIZE: u16 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ColorToken {
    TextStrong,
    TextDefault,
    TextSubtle,
    TextMuted,
    TextDisabled,
    Accent,
    OnAccent,
    Success,
    Warning,
    Error,
    Info,
    Surface,
    Panel,
    Hover,
    Selected,
    Border,
    Divider,
    Chip,
    ChipText,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    #[default]
    Neutral,
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Space {
    #[default]
    None,
    Xs,
    Sm,
    Md,
    Lg,
    Xl,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Radius {
    #[default]
    None,
    Sm,
    Md,
    Lg,
    Full,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TextSize {
    Xs,
    #[default]
    Sm,
    Md,
    Lg,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Weight {
    #[default]
    Regular,
    Medium,
    Bold,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Font {
    #[default]
    Interface,
    Mono,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    #[default]
    Row,
    Column,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    Start,
    #[default]
    Center,
    End,
    Stretch,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Justify {
    #[default]
    Start,
    Center,
    End,
    Between,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Wrap {
    #[default]
    NoWrap,
    Wrap,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Size {
    #[default]
    Auto,
    Fill,
    Fixed(u16),
}

impl Size {
    #[must_use]
    pub fn bounded(self) -> Self {
        match self {
            Self::Fixed(pixels) => Self::Fixed(pixels.min(MAX_FIXED_SIZE)),
            other => other,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Style {
    pub padding: Space,
    pub gap: Space,
    pub background: Option<ColorToken>,
    pub border: Option<ColorToken>,
    pub radius: Radius,
    pub width: Size,
    pub height: Size,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct TextStyle {
    pub size: TextSize,
    pub weight: Weight,
    pub font: Font,
    pub color: Option<ColorToken>,
    pub wrap: Wrap,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_sizes_are_capped() {
        assert_eq!(Size::Fixed(u16::MAX).bounded(), Size::Fixed(MAX_FIXED_SIZE));
        assert_eq!(Size::Fixed(12).bounded(), Size::Fixed(12));
        assert_eq!(Size::Fill.bounded(), Size::Fill);
    }
}
