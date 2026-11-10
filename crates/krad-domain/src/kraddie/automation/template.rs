use super::Trigger;
use serde::{Deserialize, Serialize};
use std::fmt;

const BUILT_IN: &str = include_str!("templates.json");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplateCategory {
    Popular,
    CodeReview,
    Security,
    IncidentsAndTriage,
    DataAndResearch,
    Environment,
}

impl TemplateCategory {
    pub const ALL: [Self; 6] = [
        Self::Popular,
        Self::CodeReview,
        Self::Security,
        Self::IncidentsAndTriage,
        Self::DataAndResearch,
        Self::Environment,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Popular => "Crowd pleasers",
            Self::CodeReview => "Code health",
            Self::Security => "Safety net",
            Self::IncidentsAndTriage => "Firefighting",
            Self::DataAndResearch => "Insight",
            Self::Environment => "Housekeeping",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplateIcon {
    Bug,
    Shield,
    Docs,
    Tests,
    Review,
    Dependencies,
    Incident,
    Flag,
    Research,
    Environment,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomationTemplate {
    pub id: String,
    pub categories: Vec<TemplateCategory>,
    pub icon: TemplateIcon,
    pub title: String,
    pub description: String,
    pub triggers: Vec<Trigger>,
    pub instructions: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TemplateCatalogError {
    Unreadable(String),
    RepeatedId(String),
    Incomplete(String),
}

impl fmt::Display for TemplateCatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable(reason) => write!(f, "The template catalog is not valid: {reason}"),
            Self::RepeatedId(id) => write!(f, "Two templates are named {id}"),
            Self::Incomplete(id) => write!(f, "The template {id} lacks a category or a trigger"),
        }
    }
}

impl std::error::Error for TemplateCatalogError {}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TemplateCatalog {
    pub templates: Vec<AutomationTemplate>,
}

impl TemplateCatalog {
    /// # Errors
    /// Fails when the text is not a list of templates, two share an id, or one has no
    /// category, trigger or instructions.
    pub fn parse(text: &str) -> Result<Self, TemplateCatalogError> {
        let templates: Vec<AutomationTemplate> = serde_json::from_str(text)
            .map_err(|error| TemplateCatalogError::Unreadable(error.to_string()))?;
        for (at, template) in templates.iter().enumerate() {
            if templates[..at].iter().any(|seen| seen.id == template.id) {
                return Err(TemplateCatalogError::RepeatedId(template.id.clone()));
            }
            let incomplete = template.categories.is_empty()
                || template.triggers.is_empty()
                || template.instructions.trim().is_empty();
            if incomplete {
                return Err(TemplateCatalogError::Incomplete(template.id.clone()));
            }
        }
        Ok(Self { templates })
    }

    /// # Errors
    /// Fails when the catalog shipped with the app is not valid.
    pub fn built_in() -> Result<Self, TemplateCatalogError> {
        Self::parse(BUILT_IN)
    }

    pub fn in_category(
        &self,
        category: TemplateCategory,
    ) -> impl Iterator<Item = &AutomationTemplate> {
        self.templates
            .iter()
            .filter(move |template| template.categories.contains(&category))
    }

    #[must_use]
    pub fn find(&self, id: &str) -> Option<&AutomationTemplate> {
        self.templates.iter().find(|template| template.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_in_catalog_parses_and_fills_every_category() {
        let catalog = TemplateCatalog::built_in().unwrap_or_else(|error| unreachable!("{error}"));
        for category in TemplateCategory::ALL {
            assert!(
                catalog.in_category(category).count() >= 2,
                "{} has too few templates",
                category.label()
            );
        }
        let sweep = catalog
            .find("morning-risk-sweep")
            .unwrap_or_else(|| unreachable!());
        assert!(sweep.categories.contains(&TemplateCategory::Popular));
        assert!(sweep
            .triggers
            .iter()
            .any(|trigger| trigger.schedule().is_some()));
        let responder = catalog
            .find("red-build-responder")
            .unwrap_or_else(|| unreachable!());
        assert_eq!(responder.triggers.len(), 2);
        assert!(responder
            .triggers
            .iter()
            .all(|trigger| trigger.code_host().is_some()));
        assert_eq!(catalog.templates.len(), 16);
    }

    #[test]
    fn broken_catalogs_are_refused() {
        assert!(matches!(
            TemplateCatalog::parse("{"),
            Err(TemplateCatalogError::Unreadable(_))
        ));
        let one = r#"{"id":"a","categories":["security"],"icon":"bug","title":"A","description":"","triggers":[{"kind":"manual"}],"instructions":"Go"}"#;
        assert_eq!(
            TemplateCatalog::parse(&format!("[{one},{one}]")),
            Err(TemplateCatalogError::RepeatedId("a".to_owned()))
        );
        let bare = one.replace(r#"[{"kind":"manual"}]"#, "[]");
        assert_eq!(
            TemplateCatalog::parse(&format!("[{bare}]")),
            Err(TemplateCatalogError::Incomplete("a".to_owned()))
        );
        assert_eq!(
            TemplateCatalog::parse(&format!("[{one}]")).map(|catalog| catalog.templates.len()),
            Ok(1)
        );
    }
}
