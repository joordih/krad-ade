use serde::{Deserialize, Serialize};

const NAME_VAR: &str = "{{name}}";
const PACKAGE_VAR: &str = "{{package}}";
const CURSOR_MARK: &str = "$0";
const PACKAGE_SEPARATOR: char = '.';
const FOLDER_SEPARATOR: char = '/';
const PACKAGE_GAP: &str = "\n\n";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanguageTemplate {
    pub id: String,
    pub label: String,
    pub group: String,
    pub file: String,
    pub body: String,
    #[serde(default)]
    pub package_line: Option<String>,
    #[serde(default)]
    pub markers: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedTemplate {
    pub folders: Vec<String>,
    pub file: String,
    pub text: String,
    pub cursor: (u32, u32),
}

fn cursor_at(text: &str, offset: usize) -> (u32, u32) {
    let before = &text[..offset];
    let line = before.matches('\n').count();
    let column = before
        .rsplit('\n')
        .next()
        .map_or(0, |last| last.encode_utf16().count());
    (
        u32::try_from(line).unwrap_or(u32::MAX),
        u32::try_from(column).unwrap_or(u32::MAX),
    )
}

impl LanguageTemplate {
    #[must_use]
    pub fn creates(&self, extension: &str) -> bool {
        self.file
            .rsplit_once('.')
            .is_some_and(|(_, made)| made == extension)
    }

    #[must_use]
    pub fn render(&self, typed: &str, folder_package: Option<&str>) -> Option<RenderedTemplate> {
        let separator = if folder_package.is_some() {
            PACKAGE_SEPARATOR
        } else {
            FOLDER_SEPARATOR
        };
        let parts: Vec<&str> = typed.trim().split(separator).map(str::trim).collect();
        if parts.iter().any(|part| part.is_empty()) {
            return None;
        }
        let (name, folders) = parts.split_last()?;
        let package = folder_package.map(|base| {
            std::iter::once(base)
                .filter(|base| !base.is_empty())
                .chain(folders.iter().copied())
                .collect::<Vec<_>>()
                .join(&PACKAGE_SEPARATOR.to_string())
        });
        let header = self
            .package_line
            .as_ref()
            .zip(package.as_ref().filter(|package| !package.is_empty()))
            .map_or_else(String::new, |(line, package)| {
                format!("{}{PACKAGE_GAP}", line.replace(PACKAGE_VAR, package))
            });
        let marked = format!("{header}{}", self.body.replace(NAME_VAR, name));
        let offset = marked.find(CURSOR_MARK).unwrap_or(marked.len());
        let cursor = cursor_at(&marked, offset);
        let text = marked.replacen(CURSOR_MARK, "", 1);
        Some(RenderedTemplate {
            folders: folders.iter().map(|folder| (*folder).to_owned()).collect(),
            file: self.file.replace(NAME_VAR, name),
            text,
            cursor,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kotlin_class() -> LanguageTemplate {
        LanguageTemplate {
            id: "class".to_owned(),
            label: "Class".to_owned(),
            group: "Kotlin Class".to_owned(),
            file: "{{name}}.kt".to_owned(),
            body: "class {{name}} {\n    $0\n}\n".to_owned(),
            package_line: Some("package {{package}}".to_owned()),
            markers: Vec::new(),
        }
    }

    #[test]
    fn a_template_creates_files_with_the_extension_of_its_file_pattern() {
        assert!(kotlin_class().creates("kt"));
        assert!(!kotlin_class().creates("java"));
    }

    #[test]
    fn a_class_in_a_package_gets_its_package_line_its_folders_and_the_cursor_inside() {
        let rendered = kotlin_class().render("crypto.Cipher", Some("dev.neuralabs"));
        assert_eq!(
            rendered,
            Some(RenderedTemplate {
                folders: vec!["crypto".to_owned()],
                file: "Cipher.kt".to_owned(),
                text: "package dev.neuralabs.crypto\n\nclass Cipher {\n    \n}\n".to_owned(),
                cursor: (3, 4),
            })
        );
    }

    #[test]
    fn the_default_package_has_no_package_line_and_folders_split_by_slashes_elsewhere() {
        let rendered = kotlin_class().render("Main", Some(""));
        assert_eq!(
            rendered.map(|rendered| rendered.text),
            Some("class Main {\n    \n}\n".to_owned())
        );
        let component = LanguageTemplate {
            file: "{{name}}.tsx".to_owned(),
            body: "export function {{name}}() {\n  return <div>$0</div>;\n}\n".to_owned(),
            package_line: None,
            ..kotlin_class()
        };
        let rendered = component.render("ui/Button", None);
        assert_eq!(
            rendered
                .as_ref()
                .map(|rendered| (rendered.folders.clone(), rendered.cursor)),
            Some((vec!["ui".to_owned()], (1, 14)))
        );
        assert_eq!(component.render("ui//Button", None), None);
    }
}
