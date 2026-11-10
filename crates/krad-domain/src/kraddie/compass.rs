use std::fmt;

pub const MAX_COMPASS_CHARS: usize = 20_000;
pub const COMPASS_FILE: &str = "compass.md";
pub const LEGACY_COMPASS_FILE: &str = "soul.md";
pub const DEFAULT_COMPASS: &str = "# Compass\n\nPoint me the right way. A few lines go a long way:\n\n- what matters most in this project\n- what I should leave alone\n- how you like to hear back from me\n";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompassTooLong;

impl fmt::Display for CompassTooLong {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "The compass is longer than {MAX_COMPASS_CHARS} characters"
        )
    }
}

impl std::error::Error for CompassTooLong {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KraddieCompass(String);

impl Default for KraddieCompass {
    fn default() -> Self {
        Self(DEFAULT_COMPASS.to_owned())
    }
}

impl TryFrom<String> for KraddieCompass {
    type Error = CompassTooLong;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        if text.chars().count() > MAX_COMPASS_CHARS {
            return Err(CompassTooLong);
        }
        Ok(Self(text))
    }
}

impl KraddieCompass {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn is_template(&self) -> bool {
        self.0.trim() == DEFAULT_COMPASS.trim()
    }

    #[must_use]
    pub fn standing(&self) -> Option<&str> {
        let text = self.0.trim();
        (!text.is_empty() && !self.is_template()).then_some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_compass_is_the_template_and_stands_for_nothing() {
        let compass = KraddieCompass::default();
        assert!(compass.as_str().starts_with("# Compass\n\nPoint me"));
        assert!(compass.is_template());
        assert_eq!(compass.standing(), None);
        let blank = KraddieCompass::try_from("  \n".to_owned()).unwrap_or_else(|_| unreachable!());
        assert_eq!(blank.standing(), None);
        let written = KraddieCompass::try_from("# Compass\n\nAlways run the tests.\n".to_owned())
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(
            written.standing(),
            Some("# Compass\n\nAlways run the tests.")
        );
    }

    #[test]
    fn a_compass_is_bounded() {
        let long = "a".repeat(MAX_COMPASS_CHARS + 1);
        assert_eq!(KraddieCompass::try_from(long), Err(CompassTooLong));
        assert!(KraddieCompass::try_from("a".repeat(MAX_COMPASS_CHARS)).is_ok());
    }
}
