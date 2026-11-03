use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

const LOCATION_SEPARATOR: char = ':';
const PARTIAL_NOTE: &str =
    "(partial: the language server is still indexing; retry for complete results)";
const NOTHING: &str = "Nothing found";

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CodeError {
    #[error("Unknown code tool {0}")]
    UnknownTool(String),
    #[error("A code tool needs a file")]
    NeedsFile,
    #[error("{0} needs FILE:LINE:COLUMN")]
    NeedsPosition(CodeTool),
    #[error("rename needs the new name")]
    NeedsName,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CodeTool {
    Diagnostics,
    Definition,
    References,
    Hover,
    Rename,
}

impl CodeTool {
    pub const ALL: [Self; 5] = [
        Self::Diagnostics,
        Self::Definition,
        Self::References,
        Self::Hover,
        Self::Rename,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Diagnostics => "diagnostics",
            Self::Definition => "definition",
            Self::References => "references",
            Self::Hover => "hover",
            Self::Rename => "rename",
        }
    }

    #[must_use]
    pub const fn needs_position(self) -> bool {
        !matches!(self, Self::Diagnostics)
    }

    #[must_use]
    pub const fn modifies(self) -> bool {
        matches!(self, Self::Rename)
    }

    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::Diagnostics => "Compiler errors and inspection warnings of a file, from the editor's language server",
            Self::Definition => "Where the symbol at a position of a file is declared",
            Self::References => "Every use of the symbol at a position of a file across the project",
            Self::Hover => "Type, signature and documentation of the symbol at a position of a file",
            Self::Rename => "Rename the symbol at a position of a file everywhere it is used in the group",
        }
    }
}

impl fmt::Display for CodeTool {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

impl FromStr for CodeTool {
    type Err = CodeError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|tool| tool.name() == text)
            .ok_or_else(|| CodeError::UnknownTool(text.to_owned()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodePosition {
    pub line: u32,
    pub character: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeRequest {
    pub tool: CodeTool,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<CodePosition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeLine {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<CodePosition>,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeAnswer {
    pub lines: Vec<CodeLine>,
    pub partial: bool,
}

impl CodeAnswer {
    #[must_use]
    pub fn render(&self) -> String {
        let mut rendered: Vec<String> = self
            .lines
            .iter()
            .map(|line| match (&line.path, line.position) {
                (Some(path), Some(at)) => {
                    format!("{path}:{}:{} {}", at.line + 1, at.character + 1, line.text)
                }
                (Some(path), None) => format!("{path} {}", line.text),
                _ => line.text.clone(),
            })
            .collect();
        if rendered.is_empty() {
            rendered.push(NOTHING.to_owned());
        }
        if self.partial {
            rendered.push(PARTIAL_NOTE.to_owned());
        }
        rendered.join("\n")
    }
}

/// # Errors
///
/// Returns [`CodeError::NeedsFile`] for an empty target.
pub fn parse_target(text: &str) -> Result<(String, Option<CodePosition>), CodeError> {
    let mut parts = text.rsplitn(3, LOCATION_SEPARATOR);
    let last = parts.next().unwrap_or_default();
    let middle = parts.next();
    let first = parts.next();
    match (
        first,
        middle.and_then(|line| line.parse::<u32>().ok()),
        last.parse::<u32>().ok(),
    ) {
        (Some(path), Some(line), Some(column)) if line > 0 && column > 0 => Ok((
            path.to_owned(),
            Some(CodePosition {
                line: line - 1,
                character: column - 1,
            }),
        )),
        _ if text.is_empty() => Err(CodeError::NeedsFile),
        _ => Ok((text.to_owned(), None)),
    }
}

impl CodeRequest {
    /// # Errors
    ///
    /// Returns [`CodeError`] when the target has no file, the tool needs a position the target
    /// lacks, or a rename has no new name.
    pub fn new(tool: CodeTool, target: &str, new_name: Option<String>) -> Result<Self, CodeError> {
        let (path, position) = parse_target(target)?;
        if tool.needs_position() && position.is_none() {
            return Err(CodeError::NeedsPosition(tool));
        }
        if tool == CodeTool::Rename && new_name.as_deref().is_none_or(str::is_empty) {
            return Err(CodeError::NeedsName);
        }
        Ok(Self {
            tool,
            path,
            position,
            new_name,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_read_one_based_lines_and_columns() -> Result<(), CodeError> {
        let (path, position) = parse_target("src/Main.java:12:5")?;
        assert_eq!(path, "src/Main.java");
        assert_eq!(
            position,
            Some(CodePosition {
                line: 11,
                character: 4
            })
        );
        assert_eq!(
            parse_target("src/Main.java")?,
            ("src/Main.java".to_owned(), None)
        );
        Ok(())
    }

    #[test]
    fn tools_that_need_a_position_refuse_a_bare_file() {
        assert!(CodeRequest::new(CodeTool::Definition, "Main.java", None).is_err());
        assert!(CodeRequest::new(CodeTool::Diagnostics, "Main.java", None).is_ok());
        assert!(CodeRequest::new(CodeTool::Rename, "Main.java:1:1", None).is_err());
    }

    #[test]
    fn answers_render_one_based_locations_and_flag_partial_ones() {
        let answer = CodeAnswer {
            lines: vec![CodeLine {
                path: Some("Main.java".to_owned()),
                position: Some(CodePosition {
                    line: 0,
                    character: 4,
                }),
                text: "Cannot resolve symbol 'x'".to_owned(),
            }],
            partial: true,
        };
        let rendered = answer.render();
        assert!(rendered.starts_with("Main.java:1:5 Cannot resolve symbol 'x'"));
        assert!(rendered.ends_with(PARTIAL_NOTE));
    }
}
