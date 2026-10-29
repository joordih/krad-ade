use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum EnvironmentView {
    #[default]
    Sessions,
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
    fn views_roundtrip() -> serde_json::Result<()> {
        for view in EnvironmentView::ALL {
            let json = serde_json::to_string(&view)?;
            assert_eq!(serde_json::from_str::<EnvironmentView>(&json)?, view);
        }
        Ok(())
    }
}
