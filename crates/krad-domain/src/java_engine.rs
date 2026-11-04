use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum JavaEngine {
    #[default]
    Jdtls,
    IntelliJ,
}

impl JavaEngine {
    pub const ALL: [Self; 2] = [Self::Jdtls, Self::IntelliJ];

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Jdtls => "Eclipse JDT",
            Self::IntelliJ => "IntelliJ (unstable)",
        }
    }

    #[must_use]
    pub const fn is_unstable(self) -> bool {
        matches!(self, Self::IntelliJ)
    }

    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stable_engine_is_the_default_and_stays_out_of_the_file() -> serde_json::Result<()> {
        assert_eq!(JavaEngine::default(), JavaEngine::Jdtls);
        assert!(JavaEngine::IntelliJ.is_unstable());
        let read: JavaEngine = serde_json::from_str(r#""IntelliJ""#)?;
        assert_eq!(read, JavaEngine::IntelliJ);
        Ok(())
    }
}
