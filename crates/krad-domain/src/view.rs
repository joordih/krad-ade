use serde::de::IntoDeserializer;
use serde::{Deserialize, Deserializer, Serialize};

#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default, Serialize, Deserialize,
)]
pub enum EnvironmentView {
    #[default]
    Sessions,
    Notes,
    Automations,
    #[serde(alias = "Pets")]
    Kraddies,
    Review,
}

impl EnvironmentView {
    pub const ALL: [Self; 5] = [
        Self::Sessions,
        Self::Notes,
        Self::Automations,
        Self::Kraddies,
        Self::Review,
    ];

    #[must_use]
    pub const fn is_optional(self) -> bool {
        !matches!(self, Self::Sessions)
    }
}

pub(crate) fn lenient_view<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<EnvironmentView, D::Error> {
    let stored = String::deserialize(deserializer)?;
    let known: Result<EnvironmentView, serde::de::value::Error> =
        EnvironmentView::deserialize(stored.as_str().into_deserializer());
    Ok(known.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEGACY_VIEW: &str = r#""Pets""#;

    const fn persisted(view: EnvironmentView) -> &'static str {
        match view {
            EnvironmentView::Sessions => r#""Sessions""#,
            EnvironmentView::Notes => r#""Notes""#,
            EnvironmentView::Automations => r#""Automations""#,
            EnvironmentView::Kraddies => r#""Kraddies""#,
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

    #[test]
    fn a_view_this_version_does_not_know_opens_sessions() -> serde_json::Result<()> {
        #[derive(Deserialize)]
        struct Saved {
            #[serde(deserialize_with = "lenient_view")]
            view: EnvironmentView,
        }
        let notes: Saved = serde_json::from_str(r#"{"view":"Notes"}"#)?;
        assert_eq!(notes.view, EnvironmentView::Notes);
        let removed: Saved = serde_json::from_str(r#"{"view":"Workflows"}"#)?;
        assert_eq!(removed.view, EnvironmentView::Sessions);
        Ok(())
    }

    #[test]
    fn the_view_saved_before_the_rename_opens_kraddies() -> serde_json::Result<()> {
        assert_eq!(
            serde_json::from_str::<EnvironmentView>(LEGACY_VIEW)?,
            EnvironmentView::Kraddies
        );
        Ok(())
    }
}
