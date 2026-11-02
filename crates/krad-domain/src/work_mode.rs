use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum WorkMode {
    #[default]
    Ade,
    Editor,
}

impl WorkMode {
    pub const ALL: [Self; 2] = [Self::Ade, Self::Editor];

    #[must_use]
    pub const fn toggled(self) -> Self {
        match self {
            Self::Ade => Self::Editor,
            Self::Editor => Self::Ade,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggling_goes_back_and_forth() {
        for mode in WorkMode::ALL {
            assert_ne!(mode.toggled(), mode);
            assert_eq!(mode.toggled().toggled(), mode);
        }
    }

    #[test]
    fn work_modes_persist_by_their_name() -> serde_json::Result<()> {
        assert_eq!(serde_json::to_string(&WorkMode::Ade)?, r#""Ade""#);
        assert_eq!(serde_json::to_string(&WorkMode::Editor)?, r#""Editor""#);
        assert_eq!(
            serde_json::from_str::<WorkMode>(r#""Editor""#)?,
            WorkMode::Editor
        );
        Ok(())
    }
}
