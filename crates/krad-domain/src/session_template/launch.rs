use crate::agent::AgentPreset;
use crate::mcp::McpServerName;
use crate::model::ModelId;
use crate::review::Issue;
use std::fmt;

use super::branch::{branch_slug, words_slug, BranchName};
use super::text::TemplateValues;
use super::SessionTemplate;

const ISSUE_BRANCH_PREFIX: &str = "issue";
const SLUG_SEPARATOR: char = '-';
const ISSUE_MARK: char = '#';
const ISSUE_PROMPT_LEAD: &str = "Work on issue";
const ISSUE_PROMPT_LINK: &str = "Issue:";
const LINK_SEPARATOR: char = ' ';
const PARAGRAPH: &str = "\n\n";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplateLaunch {
    pub session_name: String,
    pub harness: Option<AgentPreset>,
    pub model: Option<ModelId>,
    pub worktree: Option<BranchName>,
    pub mcp_servers: Option<Vec<McpServerName>>,
    pub prompt: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplateLaunchError;

impl fmt::Display for TemplateLaunchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("The template gives no usable worktree branch name")
    }
}

impl std::error::Error for TemplateLaunchError {}

#[must_use]
pub fn issue_prompt(issue: &Issue) -> String {
    let mut prompt = format!(
        "{ISSUE_PROMPT_LEAD} {ISSUE_MARK}{}: {}",
        issue.number,
        issue.title.trim()
    );
    let body = issue.body.trim();
    if !body.is_empty() {
        prompt.push_str(PARAGRAPH);
        prompt.push_str(body);
    }
    if !issue.web_url.is_empty() {
        prompt.push_str(PARAGRAPH);
        prompt.push_str(ISSUE_PROMPT_LINK);
        prompt.push(LINK_SEPARATOR);
        prompt.push_str(&issue.web_url);
    }
    prompt
}

fn issue_session_name(issue: &Issue) -> String {
    format!("{ISSUE_MARK}{} {}", issue.number, issue.title.trim())
}

fn issue_branch(issue: &Issue) -> Option<BranchName> {
    branch_slug(&format!(
        "{ISSUE_BRANCH_PREFIX}{SLUG_SEPARATOR}{}{SLUG_SEPARATOR}{}",
        issue.number,
        words_slug(&issue.title)
    ))
}

fn non_empty(text: &str) -> Option<String> {
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

impl SessionTemplate {
    /// # Errors
    ///
    /// Returns [`TemplateLaunchError`] when the template asks for a worktree
    /// and neither its branch pattern, the issue nor its names give a branch.
    pub fn launch(&self, issue: Option<&Issue>) -> Result<TemplateLaunch, TemplateLaunchError> {
        let plain = TemplateValues {
            issue,
            branch: None,
        };
        let session_name = self
            .session_name
            .as_ref()
            .and_then(|text| non_empty(&text.render(&plain)))
            .or_else(|| issue.map(issue_session_name))
            .unwrap_or_else(|| self.name.clone());
        let worktree = match &self.worktree {
            None => None,
            Some(worktree) => Some(
                worktree
                    .branch
                    .as_ref()
                    .and_then(|pattern| branch_slug(&pattern.render(&plain)))
                    .or_else(|| issue.and_then(issue_branch))
                    .or_else(|| branch_slug(&session_name))
                    .ok_or(TemplateLaunchError)?,
            ),
        };
        let values = TemplateValues {
            issue,
            branch: worktree.as_ref(),
        };
        let prompt = match &self.prompt {
            Some(prompt) => non_empty(&prompt.render(&values)),
            None => issue.map(issue_prompt),
        };
        Ok(TemplateLaunch {
            session_name,
            harness: self.harness,
            model: self.model.clone(),
            worktree,
            mcp_servers: self.mcp_servers.clone(),
            prompt,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::IssueNumber;
    use crate::session_template::TemplateError;

    fn issue(body: &str) -> Issue {
        Issue {
            number: IssueNumber(42),
            title: " Refund fails on Safari ".to_owned(),
            body: body.to_owned(),
            web_url: "https://github.com/o/r/issues/42".to_owned(),
            labels: vec!["bug".to_owned()],
            ..Issue::default()
        }
    }

    #[test]
    fn an_issue_becomes_a_prompt_with_its_title_body_and_link() {
        assert_eq!(
            issue_prompt(&issue("Steps:\n1. Pay\n")),
            "Work on issue #42: Refund fails on Safari\n\nSteps:\n1. Pay\n\n\
             Issue: https://github.com/o/r/issues/42"
        );
        assert_eq!(
            issue_prompt(&issue("  ")),
            "Work on issue #42: Refund fails on Safari\n\nIssue: https://github.com/o/r/issues/42"
        );
    }

    #[test]
    fn the_issue_template_opens_a_worktree_named_after_the_issue() -> Result<(), TemplateLaunchError>
    {
        let issue = issue("Steps");
        let launch = SessionTemplate::for_issue().launch(Some(&issue))?;
        assert_eq!(launch.session_name, "#42 Refund fails on Safari");
        assert_eq!(
            launch.worktree.as_ref().map(BranchName::as_str),
            Some("issue-42-refund-fails-on-safari")
        );
        assert_eq!(launch.prompt, Some(issue_prompt(&issue)));
        assert_eq!(launch.harness, None);
        Ok(())
    }

    #[test]
    fn a_template_fills_its_texts_and_branch_from_the_issue(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let template: SessionTemplate = r#"{
            "format": 1,
            "name": "Fix",
            "harness": "Codex",
            "model": "gpt-5",
            "worktree": {"branch": "fix/{{issue.number}} {{issue.title}}"},
            "session_name": "Fix {{issue.number}}",
            "prompt": "Fix {{issue.title}} on {{branch}}.\n{{issue.body}}"
        }"#
        .parse()
        .map_err(|error: TemplateError| error.to_string())?;
        let launch = template.launch(Some(&issue("Body")))?;
        assert_eq!(launch.session_name, "Fix 42");
        assert_eq!(launch.harness, Some(AgentPreset::Codex));
        assert_eq!(launch.model, ModelId::parse("gpt-5"));
        assert_eq!(
            launch.worktree.as_ref().map(BranchName::as_str),
            Some("fix-42-refund-fails-on-safari")
        );
        assert_eq!(
            launch.prompt.as_deref(),
            Some("Fix  Refund fails on Safari  on fix-42-refund-fails-on-safari.\nBody")
        );
        Ok(())
    }

    #[test]
    fn without_an_issue_a_template_falls_back_to_its_own_names(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let template: SessionTemplate = r#"{
            "format": 1,
            "name": "Spike",
            "worktree": {"branch": "{{issue.slug}}"},
            "prompt": "  "
        }"#
        .parse()
        .map_err(|error: TemplateError| error.to_string())?;
        let launch = template.launch(None)?;
        assert_eq!(launch.session_name, "Spike");
        assert_eq!(
            launch.worktree.as_ref().map(BranchName::as_str),
            Some("spike")
        );
        assert_eq!(launch.prompt, None);
        let unnamed: SessionTemplate = r#"{"format": 1, "name": "¿?", "worktree": {}}"#
            .parse()
            .map_err(|error: TemplateError| error.to_string())?;
        assert_eq!(unnamed.launch(None), Err(TemplateLaunchError));
        Ok(())
    }
}
