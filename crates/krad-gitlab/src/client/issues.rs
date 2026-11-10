use krad_domain::review::{
    sort_issues, Issue, IssueActivity, IssueNumber, IssuePage, IssueQuery, IssueState,
    LinkedRequest, ListPage, ProjectPath, TimelineEntry,
};

use super::{
    project_path, GitlabClient, GitlabError, AUTHORIZATION, NEXT_PAGE_HEADER, NOTES_PATH,
    PAGE_PARAM, PER_PAGE, PER_PAGE_PARAM, STATE_PARAM,
};
use crate::issue::{order, state_event, state_filter, IssueJson, IssueNoteJson, RelatedJson};
use crate::model::NoteRequest;

const ISSUES_PATH: &str = "/issues";
const RELATED_PATH: &str = "/related_merge_requests";
const AUTHOR_PARAM: &str = "author_username";
const ASSIGNEE_PARAM: &str = "assignee_username";
const LABELS_PARAM: &str = "labels";
const SEARCH_PARAM: &str = "search";
const ORDER_PARAM: &str = "order_by";
const SORT_PARAM: &str = "sort";
const NOTES_SORT: &str = "asc";
const NOTES_ORDER: &str = "created_at";

fn list_params(query: &IssueQuery) -> Vec<(&'static str, String)> {
    let (order_by, sort) = order(query.sort);
    let mut params = vec![
        (STATE_PARAM, state_filter(query.state).to_owned()),
        (ORDER_PARAM, order_by.to_owned()),
        (SORT_PARAM, sort.to_owned()),
    ];
    params.extend(query.author.clone().map(|author| (AUTHOR_PARAM, author)));
    params.extend(
        query
            .assignee
            .clone()
            .map(|assignee| (ASSIGNEE_PARAM, assignee)),
    );
    params.extend(query.label.clone().map(|label| (LABELS_PARAM, label)));
    params.extend(
        query
            .search
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(|text| (SEARCH_PARAM, text.to_owned())),
    );
    params
}

fn issue_path(project: &ProjectPath, number: IssueNumber, rest: &str) -> String {
    project_path(project, &format!("{ISSUES_PATH}/{number}{rest}"))
}

impl GitlabClient {
    /// # Errors
    ///
    /// See [`GitlabClient::current_user`].
    pub fn issues(&self, project: &ProjectPath) -> Result<Vec<Issue>, GitlabError> {
        self.issue_page(project, &IssueQuery::default(), ListPage::FIRST)
            .map(|page| page.issues)
    }

    /// # Errors
    ///
    /// See [`GitlabClient::current_user`].
    pub fn issue_page(
        &self,
        project: &ProjectPath,
        query: &IssueQuery,
        page: ListPage,
    ) -> Result<IssuePage, GitlabError> {
        let url = self.config.api_url(&project_path(project, ISSUES_PATH));
        let params = list_params(query);
        let number = page.0.to_string();
        let (listed, more) = self.authorized(|bearer| {
            let mut response = self
                .agent
                .get(&url)
                .header(AUTHORIZATION, bearer)
                .query(PER_PAGE_PARAM, PER_PAGE)
                .query(PAGE_PARAM, &number)
                .query_pairs(params.iter().map(|(key, value)| (*key, value.as_str())))
                .call()?;
            let more = response
                .headers()
                .get(NEXT_PAGE_HEADER)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| !value.is_empty());
            Ok((response.body_mut().read_json::<Vec<IssueJson>>()?, more))
        })?;
        let mut issues: Vec<Issue> = listed.into_iter().map(Issue::from).collect();
        sort_issues(&mut issues, query.sort);
        Ok(IssuePage {
            issues,
            next: more.then(|| page.next()),
        })
    }

    /// # Errors
    ///
    /// See [`GitlabClient::current_user`].
    pub fn issue(&self, project: &ProjectPath, number: IssueNumber) -> Result<Issue, GitlabError> {
        self.get::<IssueJson, _>(&issue_path(project, number, ""))
    }

    /// # Errors
    ///
    /// See [`GitlabClient::current_user`].
    pub fn issue_activity(
        &self,
        project: &ProjectPath,
        number: IssueNumber,
    ) -> Result<IssueActivity, GitlabError> {
        let timeline: Vec<TimelineEntry> = self.list::<IssueNoteJson, _>(
            &issue_path(project, number, NOTES_PATH),
            &[
                (SORT_PARAM, NOTES_SORT.to_owned()),
                (ORDER_PARAM, NOTES_ORDER.to_owned()),
            ],
        )?;
        let related: Vec<LinkedRequest> =
            self.list::<RelatedJson, _>(&issue_path(project, number, RELATED_PATH), &[])?;
        Ok(IssueActivity::new(timeline, related))
    }

    /// # Errors
    ///
    /// See [`GitlabClient::current_user`].
    pub fn comment_on_issue(
        &self,
        project: &ProjectPath,
        number: IssueNumber,
        body: &str,
    ) -> Result<(), GitlabError> {
        self.post::<IssueNoteJson, TimelineEntry>(
            &issue_path(project, number, NOTES_PATH),
            &NoteRequest { body },
        )
        .map(drop)
    }

    /// # Errors
    ///
    /// See [`GitlabClient::current_user`].
    pub fn set_issue_state(
        &self,
        project: &ProjectPath,
        number: IssueNumber,
        state: IssueState,
    ) -> Result<Issue, GitlabError> {
        let url = self.config.api_url(&issue_path(project, number, ""));
        let body = state_event(state);
        self.authorized(|bearer| {
            self.agent
                .put(&url)
                .header(AUTHORIZATION, bearer)
                .send_json(&body)?
                .body_mut()
                .read_json::<IssueJson>()
        })
        .map(Issue::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GitlabConfig;
    use krad_domain::review::{IssueSort, IssueStateFilter};
    use krad_domain::GitlabTokens;

    const ISSUES: &str = include_str!("../../fixtures/issues.json");

    #[test]
    fn a_listing_sends_state_order_people_and_search() {
        let query = IssueQuery {
            state: IssueStateFilter::Closed,
            author: Some("ana".to_owned()),
            search: Some(" token ".to_owned()),
            sort: IssueSort::Oldest,
            ..IssueQuery::default()
        };
        assert_eq!(
            list_params(&query),
            [
                ("state", "closed".to_owned()),
                ("order_by", "created_at".to_owned()),
                ("sort", "asc".to_owned()),
                ("author_username", "ana".to_owned()),
                ("search", "token".to_owned()),
            ]
        );
    }

    #[test]
    fn a_page_of_issues_asks_for_its_number_and_reads_the_next(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (host, server) =
            super::super::tests::serve_with_headers(vec![(200, ISSUES, "x-next-page: 3\r\n")])?;
        let client = GitlabClient::new(
            GitlabConfig {
                host,
                client_id: "app".to_owned(),
            },
            GitlabTokens {
                access_token: "access".to_owned(),
                refresh_token: "refresh".to_owned(),
            },
            Box::new(|_| Ok(())),
        );
        let page = client.issue_page(&"team/app".parse()?, &IssueQuery::default(), ListPage(2))?;
        assert_eq!(page.next, Some(ListPage(3)));
        assert_eq!(page.issues.len(), 2);
        let seen = server.join().map_err(|_| "server panicked")?;
        assert!(seen[0].contains("page=2"));
        assert!(seen[0].contains("state=opened"));
        Ok(())
    }
}
