use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum EnvironmentView {
    #[default]
    #[serde(
        alias = "Mosaic",
        alias = "Canvas",
        alias = "Tasks",
        alias = "Roadmap",
        alias = "Blueprint"
    )]
    Sessions,
    #[serde(alias = "Wiki")]
    Notes,
    Review,
}

impl EnvironmentView {
    pub const ALL: [Self; 3] = [Self::Sessions, Self::Notes, Self::Review];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn views_of_earlier_versions_open_sessions() -> serde_json::Result<()> {
        assert_eq!(
            serde_json::from_str::<EnvironmentView>("\"Wiki\"")?,
            EnvironmentView::Notes
        );
        for old in ["\"Mosaic\"", "\"Canvas\"", "\"Tasks\""] {
            assert_eq!(
                serde_json::from_str::<EnvironmentView>(old)?,
                EnvironmentView::Sessions
            );
        }
        for view in EnvironmentView::ALL {
            let json = serde_json::to_string(&view)?;
            assert_eq!(serde_json::from_str::<EnvironmentView>(&json)?, view);
        }
        Ok(())
    }
}
