use krad_domain::LanguageDef;

use crate::highlight::Syntax;

pub(crate) const MARKDOWN: &str = "markdown";

pub(crate) fn editor_id(language: &LanguageDef) -> Option<&str> {
    language.grammar.as_deref()
}

pub(crate) fn is_markdown(language: Option<&LanguageDef>) -> bool {
    language.is_some_and(|language| language.id.as_str() == MARKDOWN)
}

fn syntax_named(name: &str) -> Syntax {
    match name {
        "rust" => Syntax::Rust,
        "typescript" => Syntax::TypeScript,
        "python" => Syntax::Python,
        "go" => Syntax::Go,
        "c_family" => Syntax::CFamily,
        "shell" => Syntax::Shell,
        "config" => Syntax::Config,
        "json" => Syntax::Json,
        _ => Syntax::Plain,
    }
}

impl From<Option<&LanguageDef>> for Syntax {
    fn from(language: Option<&LanguageDef>) -> Self {
        language
            .and_then(|language| language.syntax.as_deref())
            .map_or(Self::Plain, syntax_named)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_extensions::builtin_catalog;
    use std::path::Path;

    #[test]
    fn the_editor_and_the_highlighter_read_the_manifest() {
        let catalog = builtin_catalog();
        let language = |path: &str| catalog.for_path(Path::new(path));
        assert_eq!(language("src/main.rs").and_then(editor_id), Some("rust"));
        assert_eq!(language("Cargo.TOML").and_then(editor_id), Some("toml"));
        assert_eq!(language("App.java").and_then(editor_id), Some("java"));
        assert_eq!(Syntax::from(language("Cargo.toml")), Syntax::Config);
        assert_eq!(Syntax::from(language("stubs.pyi")), Syntax::Python);
        assert_eq!(Syntax::from(language("app.tsx")), Syntax::TypeScript);
        assert_eq!(Syntax::from(language("Makefile")), Syntax::Plain);
        assert!(is_markdown(language("docs/README.Markdown")));
        assert!(!is_markdown(language("src/main.rs")));
    }

    #[test]
    fn web_framework_files_open_with_a_built_in_grammar() {
        let catalog = builtin_catalog();
        let grammar = |path: &str| catalog.for_path(Path::new(path)).and_then(editor_id);
        assert_eq!(grammar("src/App.vue"), Some("html"));
        assert_eq!(grammar("src/lib/Card.svelte"), Some("svelte"));
        assert_eq!(grammar("src/pages/index.astro"), Some("astro"));
        assert_eq!(grammar("styles/theme.scss"), Some("scss"));
        assert_eq!(grammar("src/Button.tsx"), Some("tsx"));
    }

    #[test]
    fn every_grammar_a_manifest_names_is_built_in() {
        use gpui_kit::component::highlighter::Language as Grammar;
        let unknown: Vec<String> = builtin_catalog()
            .iter()
            .filter_map(|language| language.grammar.clone())
            .filter(|grammar| Grammar::from_str(grammar) == Grammar::Plain)
            .collect();
        assert!(unknown.is_empty(), "{unknown:?}");
    }
}
