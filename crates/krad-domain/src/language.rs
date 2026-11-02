use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::toolchain::ToolchainId;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LanguageId(String);

impl LanguageId {
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into().to_ascii_lowercase())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for LanguageId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanguageDef {
    pub id: LanguageId,
    pub name: String,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub file_names: Vec<String>,
    #[serde(default)]
    pub line_comment: Option<String>,
    #[serde(default)]
    pub grammar: Option<String>,
    #[serde(default)]
    pub syntax: Option<String>,
    #[serde(default)]
    pub lsp_id: Option<String>,
    #[serde(default)]
    pub server: Option<String>,
    #[serde(default)]
    pub toolchains: Vec<ToolchainId>,
}

impl LanguageDef {
    #[must_use]
    pub fn lsp_id(&self) -> &str {
        self.lsp_id.as_deref().unwrap_or_else(|| self.id.as_str())
    }

    fn matches(&self, path: &Path) -> bool {
        let named = path
            .file_name()
            .map(|name| name.to_string_lossy())
            .is_some_and(|name| self.file_names.iter().any(|known| *known == name));
        let extension = path
            .extension()
            .map(|extension| extension.to_string_lossy().to_ascii_lowercase());
        named
            || extension.is_some_and(|extension| {
                self.extensions
                    .iter()
                    .any(|known| known.eq_ignore_ascii_case(&extension))
            })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LanguageCatalog {
    languages: Vec<LanguageDef>,
}

impl LanguageCatalog {
    #[must_use]
    pub fn new(languages: impl IntoIterator<Item = LanguageDef>) -> Self {
        let mut catalog = Self::default();
        for language in languages {
            catalog.add(language);
        }
        catalog
    }

    pub fn add(&mut self, language: LanguageDef) {
        match self
            .languages
            .iter_mut()
            .find(|known| known.id == language.id)
        {
            Some(known) => *known = language,
            None => self.languages.push(language),
        }
    }

    #[must_use]
    pub fn for_path(&self, path: &Path) -> Option<&LanguageDef> {
        self.languages
            .iter()
            .rev()
            .find(|language| language.matches(path))
    }

    #[must_use]
    pub fn get(&self, id: &LanguageId) -> Option<&LanguageDef> {
        self.languages.iter().find(|language| language.id == *id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &LanguageDef> {
        self.languages.iter()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerLaunch {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, OsString)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn language(id: &str, extensions: &[&str]) -> LanguageDef {
        LanguageDef {
            id: LanguageId::new(id),
            name: id.to_owned(),
            extensions: extensions
                .iter()
                .map(|extension| (*extension).to_owned())
                .collect(),
            file_names: Vec::new(),
            line_comment: None,
            grammar: None,
            syntax: None,
            lsp_id: None,
            server: None,
            toolchains: Vec::new(),
        }
    }

    #[test]
    fn a_path_finds_its_language_by_extension_in_any_case() {
        let catalog =
            LanguageCatalog::new([language("rust", &["rs"]), language("toml", &["toml"])]);
        let found = catalog
            .for_path(Path::new("src/MAIN.RS"))
            .map(|found| found.id.as_str());
        assert_eq!(found, Some("rust"));
        assert!(catalog.for_path(Path::new("LICENSE")).is_none());
    }

    #[test]
    fn a_later_definition_overrides_one_with_the_same_id() {
        let mut catalog = LanguageCatalog::new([language("rust", &["rs"])]);
        let mut changed = language("rust", &["rs", "ron"]);
        changed.line_comment = Some("//".to_owned());
        catalog.add(changed);
        assert_eq!(catalog.iter().count(), 1);
        assert!(catalog.for_path(Path::new("a.ron")).is_some());
    }

    #[test]
    fn file_names_match_files_without_an_extension() {
        let mut make = language("make", &[]);
        make.file_names = vec!["Makefile".to_owned()];
        let catalog = LanguageCatalog::new([make]);
        assert!(catalog.for_path(Path::new("api/Makefile")).is_some());
        assert_eq!(
            catalog
                .get(&LanguageId::new("MAKE"))
                .map(LanguageDef::lsp_id),
            Some("make")
        );
    }
}
