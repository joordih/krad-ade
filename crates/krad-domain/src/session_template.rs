mod branch;
mod launch;
mod text;

pub use branch::{branch_slug, BranchName};
pub use launch::{issue_prompt, TemplateLaunch, TemplateLaunchError};
pub use text::{TemplateText, TemplateTextError, TemplateValues, TemplateVariable};

use crate::agent::AgentPreset;
use crate::mcp::McpServerName;
use crate::model::ModelId;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

pub const TEMPLATES_DIR: &str = ".krad-ade/templates";
pub const TEMPLATE_EXTENSION: &str = "json";
pub const TEMPLATE_FORMAT: u32 = 1;
const ISSUE_TEMPLATE_NAME: &str = "Issue";
const ID_PUNCTUATION: &str = "-_.";
const HIDDEN_PREFIX: char = '.';

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TemplateId(String);

impl TemplateId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidTemplateId;

impl fmt::Display for InvalidTemplateId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a template file is named with letters, digits, dashes, dots and underscores")
    }
}

impl std::error::Error for InvalidTemplateId {}

impl FromStr for TemplateId {
    type Err = InvalidTemplateId;

    fn from_str(stem: &str) -> Result<Self, Self::Err> {
        let allowed = |c: char| c.is_ascii_alphanumeric() || ID_PUNCTUATION.contains(c);
        let valid =
            !stem.is_empty() && !stem.starts_with(HIDDEN_PREFIX) && stem.chars().all(allowed);
        valid
            .then(|| Self(stem.to_owned()))
            .ok_or(InvalidTemplateId)
    }
}

impl fmt::Display for TemplateId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorktreeTemplate {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<TemplateText>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "TemplateFile", into = "TemplateFile")]
pub struct SessionTemplate {
    pub name: String,
    pub harness: Option<AgentPreset>,
    pub model: Option<ModelId>,
    pub worktree: Option<WorktreeTemplate>,
    pub mcp_servers: Option<Vec<McpServerName>>,
    pub session_name: Option<TemplateText>,
    pub prompt: Option<TemplateText>,
}

impl SessionTemplate {
    #[must_use]
    pub fn for_issue() -> Self {
        Self {
            name: ISSUE_TEMPLATE_NAME.to_owned(),
            harness: None,
            model: None,
            worktree: Some(WorktreeTemplate::default()),
            mcp_servers: None,
            session_name: None,
            prompt: None,
        }
    }

    #[must_use]
    pub fn uses_issue(&self) -> bool {
        const ISSUE_VARIABLES: [TemplateVariable; 5] = [
            TemplateVariable::IssueTitle,
            TemplateVariable::IssueBody,
            TemplateVariable::IssueNumber,
            TemplateVariable::IssueUrl,
            TemplateVariable::IssueSlug,
        ];
        let branch = self
            .worktree
            .as_ref()
            .and_then(|worktree| worktree.branch.as_ref());
        [branch, self.session_name.as_ref(), self.prompt.as_ref()]
            .into_iter()
            .flatten()
            .any(|text| {
                ISSUE_VARIABLES
                    .into_iter()
                    .any(|variable| text.uses(variable))
            })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TemplateError {
    Json(String),
    Format(u32),
    EmptyName,
    NotAChatHarness(AgentPreset),
    BranchInBranch,
}

impl fmt::Display for TemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => f.write_str(error),
            Self::Format(format) => write!(
                f,
                "format {format} is not supported; this app reads format {TEMPLATE_FORMAT}"
            ),
            Self::EmptyName => f.write_str("a template needs a name"),
            Self::NotAChatHarness(harness) => {
                write!(f, "{} cannot open a chat", harness.display_name())
            }
            Self::BranchInBranch => write!(
                f,
                "the worktree branch cannot use {{{{{}}}}}",
                TemplateVariable::Branch.name()
            ),
        }
    }
}

impl std::error::Error for TemplateError {}

impl FromStr for SessionTemplate {
    type Err = TemplateError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        serde_json::from_str::<TemplateFile>(text)
            .map_err(|error| TemplateError::Json(error.to_string()))?
            .try_into()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TemplateFile {
    format: u32,
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    harness: Option<AgentPreset>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<ModelId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    worktree: Option<WorktreeTemplate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mcp_servers: Option<Vec<McpServerName>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    session_name: Option<TemplateText>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    prompt: Option<TemplateText>,
}

impl TryFrom<TemplateFile> for SessionTemplate {
    type Error = TemplateError;

    fn try_from(file: TemplateFile) -> Result<Self, Self::Error> {
        if file.format != TEMPLATE_FORMAT {
            return Err(TemplateError::Format(file.format));
        }
        let name = file.name.trim();
        if name.is_empty() {
            return Err(TemplateError::EmptyName);
        }
        if let Some(harness) = file
            .harness
            .filter(|harness| !harness.has_structured_chat())
        {
            return Err(TemplateError::NotAChatHarness(harness));
        }
        let branch = file
            .worktree
            .as_ref()
            .and_then(|worktree| worktree.branch.as_ref());
        if branch.is_some_and(|branch| branch.uses(TemplateVariable::Branch)) {
            return Err(TemplateError::BranchInBranch);
        }
        Ok(Self {
            name: name.to_owned(),
            harness: file.harness,
            model: file.model,
            worktree: file.worktree,
            mcp_servers: file.mcp_servers,
            session_name: file.session_name,
            prompt: file.prompt,
        })
    }
}

impl From<SessionTemplate> for TemplateFile {
    fn from(template: SessionTemplate) -> Self {
        Self {
            format: TEMPLATE_FORMAT,
            name: template.name,
            harness: template.harness,
            model: template.model,
            worktree: template.worktree,
            mcp_servers: template.mcp_servers,
            session_name: template.session_name,
            prompt: template.prompt,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = r##"{
        "format": 1,
        "name": "Fix an issue",
        "harness": "ClaudeCode",
        "model": "sonnet",
        "worktree": { "branch": "issue-{{issue.number}}-{{issue.slug}}" },
        "mcp_servers": ["files", "browser"],
        "session_name": "#{{issue.number}} {{issue.title}}",
        "prompt": "Fix {{issue.url}} on {{branch}}"
    }"##;

    #[test]
    fn a_template_file_parses_every_field() -> Result<(), TemplateError> {
        let template: SessionTemplate = FULL.parse()?;
        assert_eq!(template.name, "Fix an issue");
        assert_eq!(template.harness, Some(AgentPreset::ClaudeCode));
        assert_eq!(template.model, ModelId::parse("sonnet"));
        let servers: Vec<&str> = template
            .mcp_servers
            .iter()
            .flatten()
            .map(McpServerName::as_str)
            .collect();
        assert_eq!(servers, ["files", "browser"]);
        assert!(template.worktree.is_some());
        assert!(template.uses_issue());
        Ok(())
    }

    #[test]
    fn a_minimal_template_needs_only_its_format_and_name() -> Result<(), TemplateError> {
        let template: SessionTemplate = r#"{"format": 1, "name": " Review "}"#.parse()?;
        assert_eq!(template.name, "Review");
        assert_eq!(template.harness, None);
        assert_eq!(template.worktree, None);
        assert!(!template.uses_issue());
        Ok(())
    }

    #[test]
    fn a_template_round_trips_through_its_file() -> Result<(), Box<dyn std::error::Error>> {
        let template: SessionTemplate = FULL.parse()?;
        let written = serde_json::to_string(&template)?;
        assert!(written.contains(r#""format":1"#));
        assert_eq!(written.parse::<SessionTemplate>()?, template);
        Ok(())
    }

    #[test]
    fn invalid_templates_say_why() {
        let parse = |text: &str| text.parse::<SessionTemplate>();
        assert_eq!(
            parse(r#"{"format": 2, "name": "Next"}"#),
            Err(TemplateError::Format(2))
        );
        assert_eq!(
            parse(r#"{"format": 1, "name": "  "}"#),
            Err(TemplateError::EmptyName)
        );
        assert_eq!(
            parse(r#"{"format": 1, "name": "Shell", "harness": "Shell"}"#),
            Err(TemplateError::NotAChatHarness(AgentPreset::Shell))
        );
        assert_eq!(
            parse(r#"{"format": 1, "name": "Loop", "worktree": {"branch": "x-{{branch}}"}}"#),
            Err(TemplateError::BranchInBranch)
        );
        assert!(matches!(
            parse(r#"{"format": 1, "name": "Typo", "promt": "hi"}"#),
            Err(TemplateError::Json(_))
        ));
        assert!(matches!(
            parse(r#"{"format": 1, "name": "Bad", "prompt": "{{issue.author}}"}"#),
            Err(TemplateError::Json(error)) if error.contains("issue.author")
        ));
    }

    #[test]
    fn template_ids_come_from_safe_file_names() {
        assert_eq!(
            "fix-issue".parse::<TemplateId>().map(|id| id.to_string()),
            Ok("fix-issue".to_owned())
        );
        assert_eq!("../x".parse::<TemplateId>(), Err(InvalidTemplateId));
        assert_eq!(".hidden".parse::<TemplateId>(), Err(InvalidTemplateId));
    }
}
