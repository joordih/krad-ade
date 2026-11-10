use crate::review::Issue;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use super::branch::BranchName;

const OPEN: &str = "{{";
const CLOSE: &str = "}}";
const ISSUE_TITLE: &str = "issue.title";
const ISSUE_BODY: &str = "issue.body";
const ISSUE_NUMBER: &str = "issue.number";
const ISSUE_URL: &str = "issue.url";
const ISSUE_SLUG: &str = "issue.slug";
const BRANCH: &str = "branch";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TemplateVariable {
    IssueTitle,
    IssueBody,
    IssueNumber,
    IssueUrl,
    IssueSlug,
    Branch,
}

impl TemplateVariable {
    pub const ALL: [Self; 6] = [
        Self::IssueTitle,
        Self::IssueBody,
        Self::IssueNumber,
        Self::IssueUrl,
        Self::IssueSlug,
        Self::Branch,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::IssueTitle => ISSUE_TITLE,
            Self::IssueBody => ISSUE_BODY,
            Self::IssueNumber => ISSUE_NUMBER,
            Self::IssueUrl => ISSUE_URL,
            Self::IssueSlug => ISSUE_SLUG,
            Self::Branch => BRANCH,
        }
    }

    fn value(self, values: &TemplateValues<'_>) -> String {
        let issue = values.issue;
        match self {
            Self::IssueTitle => issue.map(|issue| issue.title.clone()),
            Self::IssueBody => issue.map(|issue| issue.body.clone()),
            Self::IssueNumber => issue.map(|issue| issue.number.to_string()),
            Self::IssueUrl => issue.map(|issue| issue.web_url.clone()),
            Self::IssueSlug => issue.map(|issue| super::branch::words_slug(&issue.title)),
            Self::Branch => values.branch.map(|branch| branch.as_str().to_owned()),
        }
        .unwrap_or_default()
    }
}

impl FromStr for TemplateVariable {
    type Err = TemplateTextError;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|variable| variable.name() == name)
            .ok_or_else(|| TemplateTextError::UnknownVariable(name.to_owned()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TemplateTextError {
    Unclosed,
    UnknownVariable(String),
}

impl fmt::Display for TemplateTextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unclosed => write!(f, "a variable opened with {OPEN} is never closed"),
            Self::UnknownVariable(name) => write!(f, "unknown variable {OPEN}{name}{CLOSE}"),
        }
    }
}

impl std::error::Error for TemplateTextError {}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Segment {
    Text(String),
    Variable(TemplateVariable),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TemplateValues<'a> {
    pub issue: Option<&'a Issue>,
    pub branch: Option<&'a BranchName>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct TemplateText {
    source: String,
    segments: Vec<Segment>,
}

impl TemplateText {
    #[must_use]
    pub fn uses(&self, wanted: TemplateVariable) -> bool {
        self.segments.contains(&Segment::Variable(wanted))
    }

    #[must_use]
    pub fn render(&self, values: &TemplateValues<'_>) -> String {
        self.segments
            .iter()
            .map(|segment| match segment {
                Segment::Text(text) => text.clone(),
                Segment::Variable(variable) => variable.value(values),
            })
            .collect()
    }
}

impl FromStr for TemplateText {
    type Err = TemplateTextError;

    fn from_str(source: &str) -> Result<Self, Self::Err> {
        let mut segments = Vec::new();
        let mut rest = source;
        while let Some(start) = rest.find(OPEN) {
            let (text, opened) = rest.split_at(start);
            if !text.is_empty() {
                segments.push(Segment::Text(text.to_owned()));
            }
            let inner = &opened[OPEN.len()..];
            let end = inner.find(CLOSE).ok_or(TemplateTextError::Unclosed)?;
            segments.push(Segment::Variable(inner[..end].trim().parse()?));
            rest = &inner[end + CLOSE.len()..];
        }
        if !rest.is_empty() {
            segments.push(Segment::Text(rest.to_owned()));
        }
        Ok(Self {
            source: source.to_owned(),
            segments,
        })
    }
}

impl TryFrom<String> for TemplateText {
    type Error = TemplateTextError;

    fn try_from(source: String) -> Result<Self, Self::Error> {
        source.parse()
    }
}

impl From<TemplateText> for String {
    fn from(text: TemplateText) -> Self {
        text.source
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::IssueNumber;

    fn issue() -> Issue {
        Issue {
            number: IssueNumber(42),
            title: "Refund fails on Safari".to_owned(),
            body: "Steps to reproduce".to_owned(),
            web_url: "https://github.com/o/r/issues/42".to_owned(),
            labels: Vec::new(),
            ..Issue::default()
        }
    }

    #[test]
    fn variables_are_replaced_by_the_issue_values() -> Result<(), TemplateTextError> {
        let text: TemplateText =
            "Fix #{{issue.number}} ({{ issue.slug }}): {{issue.title}}\n{{issue.body}}".parse()?;
        let issue = issue();
        let values = TemplateValues {
            issue: Some(&issue),
            branch: None,
        };
        assert_eq!(
            text.render(&values),
            "Fix #42 (refund-fails-on-safari): Refund fails on Safari\nSteps to reproduce"
        );
        assert!(text.uses(TemplateVariable::IssueBody));
        assert!(!text.uses(TemplateVariable::Branch));
        Ok(())
    }

    #[test]
    fn missing_values_render_empty_and_text_round_trips() -> Result<(), TemplateTextError> {
        let source = "On {{branch}}: {{issue.url}}.";
        let text: TemplateText = source.parse()?;
        assert_eq!(text.render(&TemplateValues::default()), "On : .");
        assert_eq!(String::from(text), source);
        Ok(())
    }

    #[test]
    fn unknown_or_unclosed_variables_are_rejected() {
        assert_eq!(
            "{{issue.author}}".parse::<TemplateText>(),
            Err(TemplateTextError::UnknownVariable(
                "issue.author".to_owned()
            ))
        );
        assert_eq!(
            "Fix {{issue.title".parse::<TemplateText>(),
            Err(TemplateTextError::Unclosed)
        );
    }
}
