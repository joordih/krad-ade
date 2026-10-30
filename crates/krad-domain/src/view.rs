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

    const fn persisted(view: EnvironmentView) -> &'static str {
        match view {
            EnvironmentView::Sessions => r#""Sessions""#,
            EnvironmentView::Notes => r#""Notes""#,
            EnvironmentView::Review => r#""Review""#,
        }
    }

    #[test]
    fn views_persist_by_their_name() -> serde_json::Result<()> {
        for view in EnvironmentView::ALL {
            assert_eq!(serde_json::to_string(&view)?, persisted(view));
            assert_eq!(
                serde_json::from_str::<EnvironmentView>(persisted(view))?,
                view
            );
        }
        Ok(())
    }
}
