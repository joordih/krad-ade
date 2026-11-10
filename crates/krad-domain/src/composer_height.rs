use serde::{Deserialize, Serialize};

pub const COMPOSER_MIN_ROWS: u16 = 2;
pub const COMPOSER_COMPACT_MAX_H: f32 = 96.0;
pub const COMPOSER_NORMAL_MAX_H: f32 = 200.0;
pub const COMPOSER_ROOMY_MAX_H: f32 = 400.0;
pub const COMPOSER_WINDOW_SHARE: f32 = 0.4;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ComposerHeight {
    Compact,
    #[default]
    Normal,
    Roomy,
}

impl ComposerHeight {
    pub const ALL: [Self; 3] = [Self::Compact, Self::Normal, Self::Roomy];

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Compact => "Compact",
            Self::Normal => "Normal",
            Self::Roomy => "Roomy",
        }
    }

    #[must_use]
    pub const fn max_height(self) -> f32 {
        match self {
            Self::Compact => COMPOSER_COMPACT_MAX_H,
            Self::Normal => COMPOSER_NORMAL_MAX_H,
            Self::Roomy => COMPOSER_ROOMY_MAX_H,
        }
    }

    #[must_use]
    pub fn max_height_in(self, window_height: f32) -> f32 {
        self.max_height().min(window_height * COMPOSER_WINDOW_SHARE)
    }

    #[must_use]
    pub fn max_rows(self, window_height: f32, line_height: f32) -> u16 {
        if !line_height.is_finite() || line_height <= 0.0 {
            return COMPOSER_MIN_ROWS;
        }
        let cap = self.max_height_in(window_height);
        (COMPOSER_MIN_ROWS..=u16::MAX)
            .take_while(|rows| f32::from(*rows) * line_height <= cap)
            .last()
            .unwrap_or(COMPOSER_MIN_ROWS)
    }

    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINE: f32 = 16.0;
    const TALL_WINDOW: f32 = 2_000.0;

    #[test]
    fn each_preset_lets_the_composer_grow_further_in_a_tall_window() {
        let rows = ComposerHeight::ALL.map(|preset| preset.max_rows(TALL_WINDOW, LINE));
        assert_eq!(rows, [6, 12, 25]);
        assert!(rows.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(ComposerHeight::default(), ComposerHeight::Normal);
    }

    #[test]
    fn a_short_window_caps_every_preset_to_its_share() {
        let short = 400.0;
        let cap = short * COMPOSER_WINDOW_SHARE;
        assert!((ComposerHeight::Roomy.max_height_in(short) - cap).abs() < f32::EPSILON);
        assert_eq!(ComposerHeight::Roomy.max_rows(short, LINE), 10);
        assert_eq!(ComposerHeight::Compact.max_rows(short, LINE), 6);
    }

    #[test]
    fn the_composer_keeps_its_minimum_rows_whatever_the_window() {
        assert_eq!(
            ComposerHeight::Roomy.max_rows(10.0, LINE),
            COMPOSER_MIN_ROWS
        );
        assert_eq!(
            ComposerHeight::Normal.max_rows(TALL_WINDOW, 0.0),
            COMPOSER_MIN_ROWS
        );
        assert_eq!(
            ComposerHeight::Normal.max_rows(TALL_WINDOW, f32::NAN),
            COMPOSER_MIN_ROWS
        );
    }

    #[test]
    fn presets_are_stored_by_lowercase_name() -> serde_json::Result<()> {
        assert_eq!(serde_json::to_string(&ComposerHeight::Roomy)?, r#""roomy""#);
        assert_eq!(
            serde_json::from_str::<ComposerHeight>(r#""compact""#)?,
            ComposerHeight::Compact
        );
        Ok(())
    }
}
