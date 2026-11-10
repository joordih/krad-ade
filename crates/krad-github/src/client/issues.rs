use krad_domain::review::{
    sort_issues, Issue, IssueActivity, IssueNumber, IssuePage, IssueQuery, IssueSort, IssueState,
    IssueStateFilter, ListPage, ProjectPath,
};

use super::{repo_path, GithubClient, GithubError, STATE_PARAM};
use crate::issue::{only_issues, state_body, state_param, IssueJson, SearchJson};
use crate::model::{CommentBody, IssueCommentJson};
use crate::timeline::{timeline_entries, TimelineJson};

const ISSUES_PATH: &str = "/issues";
const SEARCH_PATH: &str = "/search/issues";
const TIMELINE_PATH: &str = "/timeline";
const COMMENTS_PATH: &str = "/comments";
const CREATOR_PARAM: &str = "creator";
const ASSIGNEE_PARAM: &str = "assignee";
const LABELS_PARAM: &str = "labels";
const SORT_PARAM: &str = "sort";
const DIRECTION_PARAM: &str = "direction";
const ORDER_PARAM: &str = "order";
const QUERY_PARAM: &str = "q";
const CREATED_SORT: &str = "created";
const UPDATED_SORT: &str = "updated";
const COMMENTS_SORT: &str = "comments";
const DESCENDING: &str = "desc";
const ASCENDING: &str = "asc";
const REPO_QUALIFIER: &str = "repo:";
const ISSUE_QUALIFIER: &str = "is:issue";
const STATE_QUALIFIER: &str = "state:";
const AUTHOR_QUALIFIER: &str = "author:";
const ASSIGNEE_QUALIFIER: &str = "assignee:";
const LABEL_QUALIFIER: &str = "label:";
const QUOTE: char = '"';
const TERM_SEPARATOR: &str = " ";

const fn sort_params(sort: IssueSort) -> (&'static str, &'static str) {
    match sort {
        IssueSort::Newest => (CREATED_SORT, DESCENDING),
        IssueSort::Oldest => (CREATED_SORT, ASCENDING),
        IssueSort::RecentlyUpdated => (UPDATED_SORT, DESCENDING),
        IssueSort::MostCommented => (COMMENTS_SORT, DESCENDING),
    }
}

fn list_params(query: &IssueQuery) -> Vec<(&'static str, String)> {
    let (sort, direction) = sort_params(query.sort);
    let mut params = vec![
        (STATE_PARAM, state_param(query.state).to_owned()),
        (SORT_PARAM, sort.to_owned()),
        (DIRECTION_PARAM, direction.to_owned()),
    ];
    params.extend(query.author.clone().map(|author| (CREATOR_PARAM, author)));
    params.extend(
        query
            .assignee
            .clone()
            .map(|assignee| (ASSIGNEE_PARAM, assignee)),
    );
    params.extend(query.label.clone().map(|label| (LABELS_PARAM, label)));
    params
}

fn search_terms(project: &ProjectPath, query: &IssueQuery, text: &str) -> String {
    let mut terms = vec![
        format!("{REPO_QUALIFIER}{project}"),
        ISSUE_QUALIFIER.to_owned(),
    ];
    if query.state != IssueStateFilter::All {
        terms.push(format!("{STATE_QUALIFIER}{}", state_param(query.state)));
    }
    terms.extend(
        query
            .author
            .as_ref()
            .map(|author| format!("{AUTHOR_QUALIFIER}{author}")),
    );
    terms.extend(
        query
            .assignee
            .as_ref()
            .map(|assignee| format!("{ASSIGNEE_QUALIFIER}{assignee}")),
    );
    terms.extend(
        query
            .label
            .as_ref()
            .map(|label| format!("{LABEL_QUALIFIER}{QUOTE}{label}{QUOTE}")),
    );
    terms.push(text.to_owned());
    terms.join(TERM_SEPARATOR)
}

fn search_params(
    project: &ProjectPath,
    query: &IssueQuery,
    text: &str,
) -> Vec<(&'static str, String)> {
    let (sort, direction) = sort_params(query.sort);
    vec![
        (QUERY_PARAM, search_terms(project, query, text)),
        (SORT_PARAM, sort.to_owned()),
        (ORDER_PARAM, direction.to_owned()),
    ]
}

fn issue_path(
    project: &ProjectPath,
    number: IssueNumber,
    rest: &str,
) -> Result<String, GithubError> {
    repo_path(project, &format!("{ISSUES_PATH}/{number}{rest}"))
}

impl GithubClient {
    /// # Errors
    ///
    /// See [`GithubClient::current_user`].
    pub fn issues(&self, project: &ProjectPath) -> Result<Vec<Issue>, GithubError> {
        self.issue_page(project, &IssueQuery::default(), ListPage::FIRST)
            .map(|page| page.issues)
    }

    /// # Errors
    ///
    /// See [`GithubClient::current_user`].
    pub fn issue_page(
        &self,
        project: &ProjectPath,
        query: &IssueQuery,
        page: ListPage,
    ) -> Result<IssuePage, GithubError> {
        let text = query
            .search
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty());
        let (listed, more) = match text {
            Some(text) => {
                let (found, more): (SearchJson, bool) =
                    self.page_of(SEARCH_PATH, &search_params(project, query, text), page)?;
                (found.items, more)
            }
            None => self.page_of::<Vec<IssueJson>>(
                &repo_path(project, ISSUES_PATH)?,
                &list_params(query),
                page,
            )?,
        };
        let mut issues = only_issues(listed);
        sort_issues(&mut issues, query.sort);
        Ok(IssuePage {
            issues,
            next: more.then(|| page.next()),
        })
    }

    /// # Errors
    ///
    /// See [`GithubClient::current_user`].
    pub fn issue(&self, project: &ProjectPath, number: IssueNumber) -> Result<Issue, GithubError> {
        self.get::<IssueJson>(&issue_path(project, number, "")?)
            .map(Issue::from)
    }

    /// # Errors
    ///
    /// See [`GithubClient::current_user`].
    pub fn issue_activity(
        &self,
        project: &ProjectPath,
        number: IssueNumber,
    ) -> Result<IssueActivity, GithubError> {
        let listed: Vec<TimelineJson> =
            self.list(&issue_path(project, number, TIMELINE_PATH)?, &[])?;
        Ok(IssueActivity::new(timeline_entries(listed), Vec::new()))
    }

    /// # Errors
    ///
    /// See [`GithubClient::current_user`].
    pub fn comment_on_issue(
        &self,
        project: &ProjectPath,
        number: IssueNumber,
        body: &str,
    ) -> Result<(), GithubError> {
        self.send_json::<_, IssueCommentJson>(
            &issue_path(project, number, COMMENTS_PATH)?,
            &CommentBody { body },
        )
        .map(drop)
    }

    /// # Errors
    ///
    /// See [`GithubClient::current_user`].
    pub fn set_issue_state(
        &self,
        project: &ProjectPath,
        number: IssueNumber,
        state: IssueState,
    ) -> Result<Issue, GithubError> {
        self.patch_json::<_, IssueJson>(&issue_path(project, number, "")?, &state_body(state))
            .map(Issue::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_search_names_the_repository_and_every_filter() -> Result<(), Box<dyn std::error::Error>> {
        let project: ProjectPath = "octo/shop".parse()?;
        let query = IssueQuery {
            state: IssueStateFilter::Closed,
            author: Some("ana".to_owned()),
            label: Some("good first issue".to_owned()),
            sort: IssueSort::Oldest,
            ..IssueQuery::default()
        };
        assert_eq!(
            search_terms(&project, &query, "refund"),
            "repo:octo/shop is:issue state:closed author:ana label:\"good first issue\" refund"
        );
        let all = IssueQuery {
            state: IssueStateFilter::All,
            ..IssueQuery::default()
        };
        assert_eq!(
            search_terms(&project, &all, "x"),
            "repo:octo/shop is:issue x"
        );
        Ok(())
    }

    #[test]
    fn a_listing_sends_state_sort_and_people() {
        let query = IssueQuery {
            assignee: Some("bo".to_owned()),
            sort: IssueSort::MostCommented,
            ..IssueQuery::default()
        };
        assert_eq!(
            list_params(&query),
            [
                ("state", "open".to_owned()),
                ("sort", "comments".to_owned()),
                ("direction", "desc".to_owned()),
                ("assignee", "bo".to_owned()),
            ]
        );
    }
}
