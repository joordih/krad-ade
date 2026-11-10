use krad_domain::review::{Issue, IssueNumber, IssueState, IssueStateFilter};
use serde::{Deserialize, Serialize};

use crate::model::UserJson;

pub(crate) const OPEN_STATE: &str = "open";
pub(crate) const CLOSED_STATE: &str = "closed";
pub(crate) const ALL_STATE: &str = "all";

#[derive(Deserialize)]
pub(crate) struct LabelJson {
    pub(crate) name: String,
}

#[derive(Deserialize)]
pub(crate) struct PullMarkerJson {}

#[derive(Deserialize)]
pub(crate) struct TitledJson {
    pub(crate) title: String,
}

#[derive(Deserialize, Default)]
pub(crate) struct ReactionsJson {
    #[serde(default)]
    pub(crate) total_count: u32,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub(crate) enum IssueStateJson {
    Open,
    Closed,
}

impl From<IssueStateJson> for IssueState {
    fn from(state: IssueStateJson) -> Self {
        match state {
            IssueStateJson::Open => Self::Open,
            IssueStateJson::Closed => Self::Closed,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct IssueJson {
    number: u64,
    title: String,
    #[serde(default)]
    body: Option<String>,
    html_url: String,
    #[serde(default)]
    labels: Vec<LabelJson>,
    #[serde(default)]
    pull_request: Option<PullMarkerJson>,
    #[serde(default = "open_state")]
    state: IssueStateJson,
    #[serde(default)]
    user: Option<UserJson>,
    #[serde(default)]
    assignees: Vec<UserJson>,
    #[serde(default)]
    milestone: Option<TitledJson>,
    #[serde(default)]
    comments: u32,
    #[serde(default)]
    reactions: ReactionsJson,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    updated_at: String,
}

const fn open_state() -> IssueStateJson {
    IssueStateJson::Open
}

impl IssueJson {
    pub(crate) const fn is_pull_request(&self) -> bool {
        self.pull_request.is_some()
    }
}

impl From<IssueJson> for Issue {
    fn from(issue: IssueJson) -> Self {
        Self {
            number: IssueNumber(issue.number),
            title: issue.title,
            body: issue.body.unwrap_or_default(),
            web_url: issue.html_url,
            labels: issue.labels.into_iter().map(|label| label.name).collect(),
            state: issue.state.into(),
            author: issue.user.map(Into::into),
            assignees: issue.assignees.into_iter().map(Into::into).collect(),
            milestone: issue.milestone.map(|milestone| milestone.title),
            comments: issue.comments,
            reactions: issue.reactions.total_count,
            created_at: issue.created_at,
            updated_at: issue.updated_at,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct SearchJson {
    pub(crate) items: Vec<IssueJson>,
}

#[derive(Serialize)]
pub(crate) struct StateBody {
    pub(crate) state: &'static str,
}

pub(crate) const fn state_param(state: IssueStateFilter) -> &'static str {
    match state {
        IssueStateFilter::Open => OPEN_STATE,
        IssueStateFilter::Closed => CLOSED_STATE,
        IssueStateFilter::All => ALL_STATE,
    }
}

pub(crate) const fn state_body(state: IssueState) -> StateBody {
    StateBody {
        state: match state {
            IssueState::Open => OPEN_STATE,
            IssueState::Closed => CLOSED_STATE,
        },
    }
}

pub(crate) fn only_issues(listed: Vec<IssueJson>) -> Vec<Issue> {
    listed
        .into_iter()
        .filter(|issue| !issue.is_pull_request())
        .map(Issue::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ISSUES: &str = include_str!("../fixtures/issues.json");
    const DETAILED: &str = include_str!("../fixtures/issue_detail.json");
    const SEARCH: &str = include_str!("../fixtures/issue_search.json");

    #[test]
    fn listed_issues_skip_pull_requests_and_keep_their_labels() -> serde_json::Result<()> {
        let issues = only_issues(serde_json::from_str(ISSUES)?);
        let numbers: Vec<u64> = issues.iter().map(|issue| issue.number.0).collect();
        assert_eq!(numbers, [42, 44]);
        assert_eq!(issues[0].title, "Refund fails on Safari");
        assert!(issues[0].body.starts_with("Steps to reproduce:\n1."));
        assert_eq!(issues[0].web_url, "https://github.com/octo/shop/issues/42");
        assert_eq!(issues[0].labels, ["bug"]);
        assert_eq!(issues[1].body, "", "a null body reads as empty");
        assert_eq!(issues[1].labels, ["docs", "good first issue"]);
        Ok(())
    }

    #[test]
    fn an_issue_reads_its_people_milestone_and_counts() -> serde_json::Result<()> {
        let issue = Issue::from(serde_json::from_str::<IssueJson>(DETAILED)?);
        assert_eq!(issue.state, IssueState::Closed);
        assert_eq!(
            issue.author.map(|author| author.username).as_deref(),
            Some("ana")
        );
        let assignees: Vec<&str> = issue
            .assignees
            .iter()
            .map(|user| user.username.as_str())
            .collect();
        assert_eq!(assignees, ["bo", "cy"]);
        assert_eq!(issue.milestone.as_deref(), Some("v2.1"));
        assert_eq!((issue.comments, issue.reactions), (5, 3));
        assert_eq!(issue.updated_at, "2026-10-04T08:30:00Z");
        Ok(())
    }

    #[test]
    fn search_results_read_as_issues() -> serde_json::Result<()> {
        let found = only_issues(serde_json::from_str::<SearchJson>(SEARCH)?.items);
        let numbers: Vec<u64> = found.iter().map(|issue| issue.number.0).collect();
        assert_eq!(numbers, [42]);
        Ok(())
    }

    #[test]
    fn states_name_the_github_values() {
        assert_eq!(state_param(IssueStateFilter::All), "all");
        assert_eq!(state_body(IssueState::Closed).state, "closed");
        assert_eq!(state_body(IssueState::Open).state, "open");
    }
}
