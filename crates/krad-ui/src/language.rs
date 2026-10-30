use std::path::Path;

use crate::highlight::Syntax;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Language {
    Rust,
    TypeScript,
    Tsx,
    JavaScript,
    Python,
    Go,
    C,
    Cpp,
    CSharp,
    Java,
    Kotlin,
    Swift,
    Scala,
    Css,
    Scss,
    Html,
    Shell,
    PowerShell,
    Toml,
    Yaml,
    Config,
    Json,
    Markdown,
}

const EXTENSIONS: &[(&str, Language)] = &[
    ("rs", Language::Rust),
    ("ts", Language::TypeScript),
    ("mts", Language::TypeScript),
    ("cts", Language::TypeScript),
    ("tsx", Language::Tsx),
    ("js", Language::JavaScript),
    ("jsx", Language::JavaScript),
    ("mjs", Language::JavaScript),
    ("cjs", Language::JavaScript),
    ("py", Language::Python),
    ("pyi", Language::Python),
    ("go", Language::Go),
    ("c", Language::C),
    ("h", Language::C),
    ("cc", Language::Cpp),
    ("cpp", Language::Cpp),
    ("hpp", Language::Cpp),
    ("cs", Language::CSharp),
    ("java", Language::Java),
    ("kt", Language::Kotlin),
    ("swift", Language::Swift),
    ("scala", Language::Scala),
    ("css", Language::Css),
    ("scss", Language::Scss),
    ("html", Language::Html),
    ("htm", Language::Html),
    ("sh", Language::Shell),
    ("bash", Language::Shell),
    ("zsh", Language::Shell),
    ("fish", Language::Shell),
    ("ps1", Language::PowerShell),
    ("toml", Language::Toml),
    ("yaml", Language::Yaml),
    ("yml", Language::Yaml),
    ("ini", Language::Config),
    ("cfg", Language::Config),
    ("conf", Language::Config),
    ("env", Language::Config),
    ("json", Language::Json),
    ("jsonc", Language::Json),
    ("md", Language::Markdown),
    ("markdown", Language::Markdown),
];

impl Language {
    pub(crate) fn from_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_string_lossy().to_ascii_lowercase();
        EXTENSIONS
            .iter()
            .find(|(known, _)| *known == extension)
            .map(|(_, language)| *language)
    }

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::TypeScript | Self::Tsx => "TypeScript",
            Self::JavaScript => "JavaScript",
            Self::Python => "Python",
            Self::Go => "Go",
            Self::C => "C",
            Self::Cpp => "C++",
            Self::CSharp => "C#",
            Self::Java => "Java",
            Self::Kotlin => "Kotlin",
            Self::Swift => "Swift",
            Self::Scala => "Scala",
            Self::Css => "CSS",
            Self::Scss => "SCSS",
            Self::Html => "HTML",
            Self::Shell => "Shell",
            Self::PowerShell => "PowerShell",
            Self::Toml => "TOML",
            Self::Yaml => "YAML",
            Self::Config => "Config",
            Self::Json => "JSON",
            Self::Markdown => "Markdown",
        }
    }

    pub(crate) const fn editor_id(self) -> Option<&'static str> {
        match self {
            Self::Rust => Some("rust"),
            Self::TypeScript => Some("typescript"),
            Self::Tsx => Some("tsx"),
            Self::JavaScript => Some("javascript"),
            Self::Python => Some("python"),
            Self::Css => Some("css"),
            Self::Html => Some("html"),
            Self::Shell => Some("bash"),
            Self::Toml => Some("toml"),
            Self::Yaml => Some("yaml"),
            Self::Json => Some("json"),
            Self::Markdown => Some("markdown"),
            Self::Go
            | Self::C
            | Self::Cpp
            | Self::CSharp
            | Self::Java
            | Self::Kotlin
            | Self::Swift
            | Self::Scala
            | Self::Scss
            | Self::PowerShell
            | Self::Config => None,
        }
    }

    pub(crate) const fn syntax(self) -> Syntax {
        match self {
            Self::Rust => Syntax::Rust,
            Self::TypeScript | Self::Tsx | Self::JavaScript => Syntax::TypeScript,
            Self::Python => Syntax::Python,
            Self::Go => Syntax::Go,
            Self::C
            | Self::Cpp
            | Self::CSharp
            | Self::Java
            | Self::Kotlin
            | Self::Swift
            | Self::Scala
            | Self::Css
            | Self::Scss => Syntax::CFamily,
            Self::Shell | Self::PowerShell => Syntax::Shell,
            Self::Toml | Self::Yaml | Self::Config => Syntax::Config,
            Self::Json => Syntax::Json,
            Self::Html | Self::Markdown => Syntax::Plain,
        }
    }
}

impl From<Option<Language>> for Syntax {
    fn from(language: Option<Language>) -> Self {
        language.map_or(Self::Plain, Language::syntax)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn language(path: &str) -> Option<Language> {
        Language::from_path(Path::new(path))
    }

    #[test]
    fn known_extensions_name_their_language_in_any_case() {
        assert_eq!(
            language("api/index.ts").map(Language::name),
            Some("TypeScript")
        );
        assert_eq!(language("src/MAIN.RS").map(Language::name), Some("Rust"));
        assert_eq!(language("LICENSE"), None);
        assert_eq!(language("data.bin"), None);
        assert_eq!(language("notes.txt"), None);
    }

    #[test]
    fn the_editor_and_the_highlighter_read_the_same_extensions() {
        assert_eq!(
            language("src/main.rs").and_then(Language::editor_id),
            Some("rust")
        );
        assert_eq!(
            language("Cargo.TOML").and_then(Language::editor_id),
            Some("toml")
        );
        assert_eq!(
            language("app.tsx").and_then(Language::editor_id),
            Some("tsx")
        );
        assert_eq!(language("main.go").and_then(Language::editor_id), None);
        assert_eq!(Syntax::from(language("Cargo.toml")), Syntax::Config);
        assert_eq!(Syntax::from(language("stubs.pyi")), Syntax::Python);
        assert_eq!(Syntax::from(language("app.tsx")), Syntax::TypeScript);
        assert_eq!(Syntax::from(language("Makefile")), Syntax::Plain);
    }

    #[test]
    fn every_extension_is_listed_once() {
        for (index, (extension, _)) in EXTENSIONS.iter().enumerate() {
            assert!(
                !EXTENSIONS[index + 1..]
                    .iter()
                    .any(|(other, _)| other == extension),
                "{extension} is listed twice"
            );
        }
    }
}
